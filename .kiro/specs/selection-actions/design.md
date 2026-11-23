# Selection Actions: design

## Flow
```mermaid
sequenceDiagram
  participant App as Any app
  participant N as bob-native (Swift)
  participant E as selection engine (Rust)
  participant B as Selection brain
  App->>N: user presses Cmd+Shift+J (Carbon hotkey)
  N->>N: read AXSelectedText (or Cmd+C, clipboard restored)
  N->>E: selection {text, secure, source, anchor}
  E->>N: show_menu {items, anchor}
  N->>N: Up/Down/Enter/Esc via the event tap (menu mode)
  N->>E: menu_choice {index}
  E->>N: menu_status "Fixing grammar…"
  E->>B: chat (BOB instructions + task + tone/language + profile/context if allowed)
  B-->>E: streamed tokens (previews every 150 ms)
  E->>E: clean (preambles, quotes, fences; keep original whitespace)
  E->>N: replace_selection {expected, text}
  N->>N: same app? selection unchanged? AX write, else paste (clipboard restored)
  N->>E: replace_result {ok, method}
  E->>N: menu_status "Done. Cmd+Z undoes it." then hide_menu
```

## Components
- **Native**
  - `HotkeyCenter`: Carbon `RegisterEventHotKey`. Default Cmd+Shift+J (spec); the core pushes the user's choice
    after every handshake. It must include Cmd, Control or Option.
  - `SelectionIO.read`: `AXSelectedText` first. If that is empty, Cmd+C (marked as a BOB event) with
    `PasteboardSnapshot` save and restore; the clipboard is untouched when nothing gets copied.
  - Secure fields are never read (SA-10).
  - `ActionMenuPanel`: a non-activating, never-key floating panel below the selection (or at the mouse), with
    clickable rows. After a choice it becomes a status line showing progress, a streamed preview or an error.
  - `MenuNavigator` (pure) handles wrap-around navigation.
  - `KeyDecision` menu mode swallows exactly plain Up, Down, Return, keypad Enter and Esc while the menu is open
    (P3). It takes precedence over Ghost capture.
  - `SelectionIO.replace`: only into the same app (pid). `SelectionGuard` compares the current selection with the
    expected one (P2), tolerating only line-ending and trailing-newline differences. The AX `AXSelectedText` write
    is accepted when the field's value changed; otherwise the helper pastes with the clipboard restored.
- **Rust**
  - `selection::actions`: built-in actions, custom actions (max 12, sanitized), hotkey setting
    (`selection-settings.json`) and token budgets.
  - `selection::clean`: the output cleaner (P5).
  - `selection::engine`: the state machine `Idle -> Menu -> Running -> Replacing -> Idle`, the 90 s deadline,
    stats, and auto-hide timers guarded by a generation counter.
  - Prompts come from `ContextHub::chat_system_opts` (personal-context spec): BOB's base instructions, the task,
    tone, language (left out for Translate), and profile and visible context only for brains allowed to receive
    them (PC-6).
- **Default brain:** whatever has the Selection role (the built-in Qwen2.5-1.5B-Instruct unless the user picks
  another). The 3B candidate was dropped: its license is not Apache-2.0 (see Task 5 notes).

## Correctness properties
- **P1 (SA-2):** clipboard save/restore is the identity on the pasteboard contents (Swift test on a private
  named pasteboard).
- **P2 (SA-7):** replacement is never issued when the current selection differs from the captured one
  (`SelectionGuard` tests), when the app changed, or after the menu was cancelled (engine test).
- **P3 (SA-4):** the key-capture predicate swallows exactly {Up, Down, Enter, Esc} while the menu is visible and
  nothing otherwise (exhaustive Swift test over all key codes and modifiers).
- **P4 (SA-9):** text with credential-like tokens is never sent to a cloud brain. The action is refused, because
  redacting would write "[redacted]" into the user's document (engine test).
- **P5:** the output cleaner is idempotent and never returns more text than the model produced (property test).

## Risks
- Apps that ignore AX replacement (Electron): paste fallback.
- Large selections: capped at 8,000 characters with a message.
- Hotkey conflicts (Chrome uses Cmd+Shift+J for Downloads): the hotkey is configurable in the Overview card.

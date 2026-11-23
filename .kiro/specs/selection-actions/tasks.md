# Selection Actions: tasks
Plan task 7. Tick a box in the same commit as the code.
- [x] 1. Action definitions, prompt templates, output cleaner with property test P5 (_SA-3_): `selection::{actions,clean}`; built-ins Fix grammar, Rephrase, Make shorter, Make friendlier, Make professional, Summarize, Translate (target from Personalize > Language), plus up to 12 custom actions
- [x] 2. Native: hotkey, selection reader (AX + Cmd+C fallback with clipboard restore, P1) (_SA-1, SA-2, SA-10_): `HotkeyCenter` (Carbon, default Cmd+Shift+J), `SelectionIO.read`, `PasteboardSnapshot`
- [x] 3. Native: action menu panel and key capture predicate (P3) (_SA-1, SA-4_): `ActionMenuPanel`, `MenuNavigator`, `KeyDecision` menu mode
- [x] 4. Engine: state machine, streaming, replace guard P2, redaction P4 (_SA-5 to SA-9_): `selection::engine`; text with credential-like tokens is refused for cloud brains instead of redacted (redacting would write "[redacted]" into the document); `SelectionGuard` in the helper
- [x] 5. Replace in place with paste fallback (_SA-6_): AX `AXSelectedText` write, verified by a value change, else paste with clipboard restore
  - [ ] 5.1 Owner check: undo (Cmd+Z) as one step in TextEdit, Notes, Mail; Chrome/Slack paste path
- [x] 6. Settings: hotkey presets and custom actions (Overview > Selection Actions); default brain is the Selection role in Brains; tone, language and profile come from Personalize (`chat_system`)
- [x] 7. Docs and journal

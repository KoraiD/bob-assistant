# Selection Actions: requirements

Required capability. Highlight text in any app, press a hotkey, transform it, replace in place. EARS IDs `SA-n`.

## User stories
- As a writer, I want to rephrase, fix, summarize or translate highlighted text without copy/paste.
- As a keyboard user, I want to drive the action menu with arrows, Enter and Esc.

## Requirements
- **SA-1** WHEN the user presses the Selection Actions hotkey (default Cmd+Shift+J, configurable) AND text is selected, THE System SHALL show a native, non-activating action menu near the selection.
- **SA-2** THE System SHALL read the selection via Accessibility and, WHERE unavailable, via Cmd+C with clipboard save and restore.
- **SA-3** THE System SHALL provide built-in actions: Rephrase, Fix grammar, Summarize, Translate, and Custom prompt.
- **SA-4** WHILE the menu is visible, THE System SHALL capture Up, Down, Enter and Esc, and SHALL swallow no other keys.
- **SA-5** WHEN an action is chosen, THE System SHALL stream the result from the selection brain and show progress.
- **SA-6** WHEN the result completes, THE System SHALL replace the selection in place via AX replacement, falling back to paste with clipboard restore.
- **SA-7** IF the selection changed, the app lost focus or the brain failed before replacement, THEN THE System SHALL NOT modify the document and SHALL tell the user why.
- **SA-8** THE System SHALL let the user undo with the app's normal Cmd+Z after replacement (single replace operation).
- **SA-9** THE System SHALL redact credential-like tokens before sending text to a cloud brain and SHALL label cloud use.
- **SA-10** WHEN the focused element is a secure text field, THE System SHALL NOT read the selection.

## Out of scope
Multi-selection, rich-text formatting preservation beyond plain text.

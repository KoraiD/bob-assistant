---
inclusion: fileMatch
fileMatchPattern: ["native/**/*.swift", "native/**/Package.swift", "docs/COMPATIBILITY.md"]
---

# macOS native layer guidance

Some notes were written from memory and must be validated in the Task 2 spike; record results in
`docs/COMPATIBILITY.md` and update this file when a note turns out wrong.

- **Caret rect:** `kAXBoundsForRangeParameterizedAttribute` on the focused element, with the selected
  text range. Result is in screen coordinates (top-left origin); flip for AppKit. If bounds are
  unavailable or implausible, **hide the chip** rather than misplace it.
- **Observation:** use `AXObserver` (focus changed, value changed, selected text changed) instead of polling.
- **Chromium/Electron:** set `AXManualAccessibility` on the app element to expose the tree. `AXBoundsForRange` is
  unreliable there (verified 2026-11-22); use the text-marker API (`AXSelectedTextMarkerRange` +
  `AXBoundsForTextMarkerRange`). Chromium builds those rects in AppKit coordinates, so always validate a caret rect against
  the element frame and accept the orientation that fits (`CaretOrientation`, `OrientationMemory`).
- **Event tap:** active `CGEventTap` needs Accessibility permission. The callback must be fast: read an atomic
  "chip visible" flag, swallow Tab/Shift+Tab/Esc/arrows/Enter only when it is set. Re-enable the tap on
  `tapDisabledByTimeout` and `tapDisabledByUserInput`.
- **Secure input:** event taps do not see keys in secure fields. Also treat `AXSecureTextField` as a quiet zone.
- **Insertion order:** synthetic Unicode key events (`CGEventKeyboardSetUnicodeString`) first, then AX selected-text
  replacement, then Cmd+V with clipboard save/restore.
- **Overlay:** transparent, non-activating, click-through `NSPanel` (`.nonactivatingPanel`, `ignoresMouseEvents`,
  floating level, `canJoinAllSpaces` + `fullScreenAuxiliary`). Never steal focus.
- **TCC for CLI tools:** a command-line binary gets the Accessibility trust of the app hosting its terminal (iTerm,
  Terminal, Kiro), not its own entry (verified 2026-11-22). Grant the host app and restart it.
- **TCC:** permission grants are tied to the code-signing identity. Sign dev builds with a stable identity,
  otherwise grants reset on every rebuild. Verify attribution for the nested helper binary.
- **Concurrency:** AX and AppKit calls on the main actor; heavy work off-main; no force unwraps on AX values.

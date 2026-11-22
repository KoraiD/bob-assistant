# Compatibility matrix

Filled in by the Task 2 spike and extended by Tasks 4 to 8. Each cell records what was **observed on real hardware**,
not what we expect. Cells marked `-` are not tested yet.

Test machine: Apple M4 Max, macOS 27.2 (record the final machine before submission).

## How to run the spike (about 15 minutes)

The helper has two standalone modes that need no Rust and print **no typed text** (only lengths and geometry):

```bash
make native        # build the helper
make probe         # observe the focused field for 20 s, print one JSON line per change + a summary
make chip-demo     # show a ghost chip at the caret in whatever app you type in (Ctrl+C to stop)
```

1. **Grant Accessibility to the terminal app, not to `bob-native`.** macOS attributes a command-line tool's
   Accessibility trust to the app that hosts the terminal (iTerm, Terminal, or Kiro if you use its integrated terminal).
   System Settings > Privacy & Security > Accessibility > "+" > pick that app > switch it on. If it is already listed but
   the probe still says "not granted", remove it with "-" and add it again. Then **quit the terminal app completely
   (Cmd+Q) and reopen it**: trust is read when the app starts. The probe prints which app it detected and opens the pane.
2. For each app in the table: run `make probe`, click into a **text field of that app within a few seconds**, type a few
   words, move the caret with the arrow keys, then read the summary line.
3. Fill the row:
   - **Caret rect**: `samples_with_caret / samples` in the summary (and whether the numbers look right: run
     `make chip-demo` and check that the chip sits at the caret).
   - **Text read**: `value_len` is not null in the sample lines.
   - **AX notifications**: which of `AXFocusedUIElementChanged`, `AXValueChanged`, `AXSelectedTextChanged` appear in
     `ax_notifications`. Apps that send none cannot be observed without polling.
   - **Secure field**: in Safari or Chrome focus a password field; the sample must show `"secure": true` and `value_len: null`.
4. Tab capture and insertion are tested in Task 4 (they need the event tap), so those columns stay `-` for now.

## Matrix

| App | Caret rect | Text read | AX notifications | Secure field quiet | Tab capture | Insert (Unicode keys) | Notes |
|---|---|---|---|---|---|---|---|
| iTerm2 (not a target app) | yes, `range_at_caret` | yes (whole buffer, capped to 2000 chars) | focus, value, selection | n/a | - | - | zero-length range returns a 34 pt line box on an empty prompt, so the chip started oversized and shrank once text was typed. Fixed by preferring the previous glyph box (2026-11-22); re-test |
| Kiro (Electron, chat input) | **yes, `text_marker`** (none with `AXBoundsForRange`) | yes | value, selection | n/a | - | - | chip sits at the caret at the right size (owner screenshot, 2026-11-22) |
| VS Code (Electron) | **yes, `text_marker`** | yes | focus, value, selection | n/a | - | - | `frame_found: false`: no AXPosition/AXSize on the element, so the rect is used without frame validation. Placement to confirm |
| Slack (Electron) | **yes, `text_marker`** | yes | focus, value, selection | n/a | - | - | empty field: collapsed range returns a wide box (x 1214 used as the right edge was wrong; one typed char gave x 556). Fixed: left edge for collapsed ranges, glyph box when a character exists (2026-11-22); re-test |
| TextEdit | - | - | - | n/a | - | - | |
| Notes | - | - | - | n/a | - | - | |
| Mail (compose) | - | - | - | n/a | - | - | |
| Safari (textarea) | - | - | - | - | - | - | |
| Chrome (textarea) | - | - | - | - | - | - | |
| VS Code | - | - | - | n/a | - | - | |
| Slack | - | - | - | n/a | - | - | |

## Open spike questions
1. **TCC attribution of the nested helper.** Run `make probe` from Terminal (grant goes to Terminal). Then run the packaged
   `BOB.app` (Task 10) and check that the **helper** shows as granted under BOB, not as a separate entry. Dev note: grants are
   tied to the signing identity, so use `make sign-dev` (identity from `BOB_SIGN_IDENTITY`, never committed) to keep grants
   across rebuilds.
2. **whisper.cpp resident server.** Not investigated yet (Task 8 decides between a bundled server and `whisper-rs`).
3. **Input Monitoring.** Whether the active event tap and modifier-only hotkeys need it in addition to Accessibility
   (Task 4 / Task 8).
4. **Which apps return usable `kAXBoundsForRangeParameterizedAttribute` values?** The matrix above answers this; the helper
   already tries three ranges (zero-length at the caret, previous character, next character) and refuses implausible rects.

## Research: caret bounds in Chromium / Electron (2026-11-22)
- Symptom: `AXBoundsForRange` fails or returns a bogus origin in Electron apps (seen in our probe for Kiro; same report in
  [electron/electron#34755](https://github.com/electron/electron/issues/34755), where every query returned x 0 and y equal to
  the screen height).
- Chromium implements the WebKit-style **text marker** API instead: `AXSelectedTextMarkerRange`, `AXBoundsForTextMarkerRange`,
  `AXPreviousTextMarkerForTextMarker` and more (`ui/accessibility/platform/browser_accessibility_cocoa.mm` in the Chromium source).
  The constants are public in the macOS SDK (`HIServices/AXWebConstants.h`), and so are `AXTextMarkerRangeCopyStartMarker` and
  `AXTextMarkerRangeCreate` (`AXUIElement.h`).
- Chromium converts the bounds with `gfx::ScreenRectToNSRect`, which yields AppKit (bottom-left) coordinates. Whether the value
  reaches us flipped depends on the system bridge, so the helper does not assume: every candidate rect is checked against the
  element's own frame (`AXPosition` + `AXSize`) and the orientation that lies inside the field wins. When both fit (a tall field
  around the middle of the screen) the last unambiguous result for that app decides (`OrientationMemory`).
- Other tools fall back to the text field frame or the mouse position when the caret is unknown (for example the MIT-licensed
  CursorBounds package). BOB does **not** do that for Ghost Text: a suggestion at the wrong spot is worse than none (GT-4).

## Implemented behavior the matrix validates
- Caret composition (`CaretRectResolver.compose`): the glyph box of the character before the caret wins when it is on the
  caret's line (tight height, exact x). Otherwise the **left edge** of the zero-length range is used (some apps return a whole
  line or placeholder box there), then the next character. Line breaks and soft-wrapped lines are detected so the chip never
  jumps to the line above. The same composition runs for the range API and for text markers. The probe reports which one
  worked as `caret_source`, and the summary counts them in `caret_sources`. All-zero, non-finite or implausibly sized rects are rejected, so the chip is hidden instead of misplaced (GT-4).
- Secure text fields (`AXSecureTextField`) are never read (GT-3).
- Chromium/Electron apps get `AXManualAccessibility` set when they become frontmost.

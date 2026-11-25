# Compatibility matrix

Filled in by the Task 2 spike and extended by Tasks 4 to 8. Each cell records what was **observed on real hardware**,
not what we expect. Cells marked `-` are not tested yet.

Test machine: Apple M4 Max, macOS 27.2 (record the final machine before submission).

## How to run the spike (about 15 minutes)

The helper has a standalone mode that need no Rust and print **no typed text** (only lengths and geometry):

```bash
make native        # build the helper
make probe         # observe the focused field for 20 s, print one JSON line per change + a summary
```

1. **Grant Accessibility to the terminal app, not to `bob-native`.** macOS attributes a command-line tool's
   Accessibility trust to the app that hosts the terminal (iTerm, Terminal, or Kiro if you use its integrated terminal).
   System Settings > Privacy & Security > Accessibility > "+" > pick that app > switch it on. If it is already listed but
   the probe still says "not granted", remove it with "-" and add it again. Then **quit the terminal app completely
   (Cmd+Q) and reopen it**: trust is read when the app starts. The probe prints which app it detected and opens the pane.
2. For each app in the table: run `make probe`, click into a **text field of that app within a few seconds**, type a few
   words, move the caret with the arrow keys, then read the summary line.
3. Fill the row:
   - **Caret rect**: `samples_with_caret / samples` in the summary (`make e2e` checks the chip in TextEdit).
   - **Text read**: `value_len` is not null in the sample lines.
   - **AX notifications**: which of `AXFocusedUIElementChanged`, `AXValueChanged`, `AXSelectedTextChanged` appear in
     `ax_notifications`. Apps that send none cannot be observed without polling.
   - **Secure field**: in Safari or Chrome focus a password field; the sample must show `"secure": true` and `value_len: null`.
4. Tab capture and insertion are tested in Task 4 (they need the event tap), so those columns stay `-` for now.

## Matrix

| App | Caret rect | Text read | AX notifications | Secure field quiet | Tab capture | Insert (Unicode keys) | Notes |
|---|---|---|---|---|---|---|---|
| iTerm2 (not a target app) | yes, `range_at_caret` | yes (whole buffer, capped to 2000 chars) | focus, value, selection | n/a | - | - | zero-length range returns a 34 pt line box on an empty prompt, so the chip started oversized and shrank once text was typed. Glyph box now used when a character precedes the caret; on an empty terminal line there is none, so the line box is still used (terminals are not a Ghost Text target) |
| Kiro (Electron, chat input) | **yes, `text_marker`** (none with `AXBoundsForRange`) | yes | value, selection | n/a | - | - | chip sits at the caret at the right size (owner screenshot, 2026-11-22) |
| VS Code (Electron) | **yes, `text_marker`** | yes | focus, value, selection | n/a | - | - | chip at the caret, correct size (owner screenshot, editor line 11). `frame_found: false`, so the rect is used without frame validation |
| Slack (Electron) | **yes, `text_marker`** | yes | focus, value, selection | n/a | - | - | empty field: collapsed range returns a wide box (x 1214 used as the right edge was wrong; one typed char gave x 556). Fixed: left edge for collapsed ranges, glyph box when a character exists; owner confirmed placement (2026-11-22) |
| TextEdit | - | - | - | n/a | - | - | |
| Notes | - | - | - | n/a | - | - | |
| Mail (compose) | - | - | - | n/a | - | - | |
| Safari (textarea) | - | - | - | - | - | - | |
| Chrome (textarea) | yes (suggestions shown at the caret) | yes | - | - | - | - | Ghost Text works (owner, 2026-11-23); probe numbers not recorded yet |
| VS Code | - | - | - | n/a | - | - | |
| Slack | - | - | - | n/a | - | - | |

## Selection Actions (Task 7)
Select text, press Cmd+Shift+J, pick "Fix grammar", then press Cmd+Z once. Record whether the menu appears next to
the selection, whether the text is replaced, and whether a single Cmd+Z restores the original. For Read and Replace,
note "ok" or what went wrong; the paste fallback briefly changes the clipboard and restores it.

| App | Menu at selection | Read | Replace | One-step undo | Notes |
|---|---|---|---|---|---|
| TextEdit | - | - | - | - | |
| Notes | - | - | - | - | |
| Mail (compose) | - | - | - | - | |
| Chrome (textarea) | - | - | - | - | Chrome uses Cmd+Shift+J for Downloads; BOB's hotkey takes it while BOB runs (change it in Overview) |
| Slack | - | - | - | - | |
| VS Code | - | - | - | - | |

## Voice Dictation (Task 8)
Hold Option+Space, say a sentence, release. Record whether the orb appears near the cursor, whether the text is right,
and the "typed ... ms after release" number from the Dictation card. On first use macOS asks for the microphone.

| App | Orb placement | Text correct | Release to text | Notes |
|---|---|---|---|---|
| TextEdit | - | - | - | |
| Notes | - | - | - | |
| Mail (compose) | - | - | - | |
| Chrome (textarea) | - | - | - | |
| Slack | - | - | - | Option+Space is also a common shortcut for other assistants; change it in the card if it clashes |

## Screen context (opt-in)
Personalize > What BOB can see > "Active window", grant Screen Recording, type in Mail with a thread open, then
Personalize > Preview: "Text visible on screen" should list the thread. Record capture + OCR time (shown in Personalize).

| Scope | App | Text recognized | ms | Notes |
|---|---|---|---|---|
| Active window | Mail | - | - | |
| All screens | Safari article on a second display | - | - | |

## Capturing screenshots and video of the chip
Taking a screenshot activates the system capture UI, which BOB used to treat as an app switch (the chip vanished). The helper
now ignores `com.apple.screencaptureui` and `com.apple.screenshot.launcher` activations (`TransientApps`). Re-test with
Cmd+Shift+4. A delayed capture also works: `screencapture -T 5 ~/Desktop/chip.png`, then click into the app within 5 s.

## Quitting BOB.app
- **Dock > Quit:** works (owner, 2026-11-23).
- **Cmd+Q:** did not work (owner, 2026-11-23). Findings: Tauri's default menu was installed with "Quit bob" (named after
  the Rust package) using `terminate:`, the same action Dock > Quit triggers, so the key equivalent was being lost
  before the menu. wry documents this class of bug: WKWebView's `performKeyEquivalent:` can consume Cmd shortcuts and
  stop them reaching the menu (comment referencing tauri-apps/tauri#9426 in `wry/src/wkwebview/class/wry_web_view.rs`).
- **First fix:** BOB installs its own menu ("BOB > Quit BOB" Cmd+Q calling `app.exit`, the path that also stops
  llama-server), and the web page catches a plain Cmd+Q itself and calls `app_quit` in case the web view swallows it.
  Tray "Quit BOB" and SIGTERM are verified. The owner reported Cmd+Q still did nothing.
- **Root cause, found 2026-11-23:**
  1. The menu item exists with the right key equivalent. Read through Accessibility: `Quit BOB`, `AXMenuItemCmdChar = Q`.
     Clicking it (System Events) quits BOB cleanly.
  2. An AppKit local key monitor (`quit_key.rs`, which sees every key event before the web view and the menu) logged
     **no** Cmd key at all while BOB was frontmost. The keys were synthetic (System Events), so this points to the
     key being filtered before BOB's process; a physical keypress with the monitor's debug log is still to be checked.
  3. The system event-tap list (`CGGetEventTapList`) shows an active key-down filter from **Vorssaint**
     (`com.vorssaint.utils`), and its preferences contain `quitProtectionQuitEnabled = 1`. Vorssaint's Quit Protection
     (added in v3.3.3) blocks a plain Cmd+Q and requires a hold, a double press or an extra modifier, per app.
  4. BOB's own tap passes every Cmd shortcut (`KeyDecision`, unit-tested).
  BOB therefore cannot receive the shortcut while Quit Protection is on. **Use the hold or double press Vorssaint
  asks for, or exclude BOB in Vorssaint > Quit Protection.** The local monitor stays as a third quit path, in case a
  web view swallows the key.

## Open spike questions
1. **TCC attribution of the nested helper.** Partly answered (2026-11-23): `make dev` and `make probe` inherit the
   terminal's grants. `make app` builds BOB.app with the helper at `Contents/MacOS/bob-native`, both signed with the same
   Developer ID and hardened runtime; launched with `open`, the helper ran from inside the bundle and connected. Whether
   macOS shows a single "BOB" entry for both processes needs the owner's grant to confirm.
   Grants are tied to the signing identity: `make app` signs with `BOB_SIGN_IDENTITY` (never committed), so grants
   survive rebuilds.
2. **whisper.cpp resident server.** Answered (2026-11-23): whisper.cpp publishes no macOS server binary (release assets:
   Windows/Linux binaries and an Apple xcframework). BOB links the official xcframework into the helper (decision log
   in ARCHITECTURE.md).
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

## Chrome: caret before text
Typing or accepting a suggestion in Chrome produced a chip that repeated text already typed (owner's screenshot,
2026-11-24). Chrome sends the value-changed notification with the new caret bounds before the value itself has
changed. The helper now re-reads the focused field 40, 120 and 300 ms after each change and sends only real changes;
the engine ignores stale text after an accept and keeps the chip in place while only the caret has moved.
`scripts/ax-lag-probe.swift` measures the lag in a page it opens itself. Owner check pending: type and accept in a
Chrome text area and a contenteditable (Gmail compose).

## Languages
With preferred languages set, suggestions in other languages are dropped (letters plus common words). Measured in
PERFORMANCE.md. Known limit: shared letters (Hungarian and German "ü") cannot separate those two.

## Findings from the end-to-end run (2026-11-25)
- **TextEdit:** the caret on the first line was reported one line too high; fixed (see PERFORMANCE.md). Rewrites
  now paste, so `⌘Z` undoes them.
- **Slack, Teams, Discord, GitHub, Notion:** while the word at the caret starts with `@`, `#` or `:`, or the line
  starts with `/`, BOB shows no suggestion, so Tab reaches the app's own list.
- **Window switches inside one app** (several Chrome or TextEdit windows): BOB now listens for focused-window
  changes and re-checks the field four times a second while a suggestion is visible.
- **Keyboard layouts:** BOB's copy and paste shortcuts look up the keys for "c" and "v" in the current layout, so
  they also work on Dvorak and other layouts.
- **Typo fixes** use the macOS spell checker. Its autocorrection adapts to how you type, so a slip like "teh" may or
  may not be offered on a given Mac; words the spell check flags ("recieve") always are.

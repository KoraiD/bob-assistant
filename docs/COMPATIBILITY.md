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

1. **Grant Accessibility** to the app that runs the command (Terminal, iTerm or your editor): System Settings >
   Privacy & Security > Accessibility. The first run triggers the prompt.
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

## Implemented behavior the matrix validates
- Caret resolution order: zero-length range at the caret, then the right edge of the previous character, then the left edge of
  the next character. All-zero, non-finite or implausibly sized rects are rejected, so the chip is hidden instead of misplaced (GT-4).
- Secure text fields (`AXSecureTextField`) are never read (GT-3).
- Chromium/Electron apps get `AXManualAccessibility` set when they become frontmost.

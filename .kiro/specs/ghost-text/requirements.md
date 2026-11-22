# Ghost Text: requirements

Priority 1 capability. Inline next-words prediction at the caret in any macOS app.
Requirements use EARS notation. IDs (`GT-n`) are referenced by tests and the design's correctness properties.

## User stories
- As a writer, I want BOB to suggest the next few words as I type so that I can finish sentences faster.
- As a privacy-conscious user, I want suggestions to work fully offline and never appear in password fields.
- As a keyboard user, I want to accept a word, accept everything, or dismiss, without leaving the keyboard.

## Requirements (EARS)

### Triggering
- **GT-1** WHEN the user types in an editable text field and pauses for the quiet gate (60 to 80 ms), THE System SHALL request a continuation from the active Ghost-capable brain using the text before the caret.
- **GT-2** WHEN a new keystroke arrives while a request is in flight, THE System SHALL cancel the in-flight request.
- **GT-3** WHILE the focused element is a secure text field or the frontmost app is on the blocklist, THE System SHALL NOT read text, send prompts or show suggestions.
- **GT-4** IF the caret rectangle cannot be determined, THEN THE System SHALL NOT show the chip.

### Display
- **GT-5** WHEN a continuation is received, THE System SHALL show it as a click-through chip anchored at the caret without taking keyboard focus.
- **GT-6** WHEN the first token streams in, THE System SHALL paint the partial chip without waiting for the full completion.
- **GT-7** WHEN the user types a character equal to the chip's next character, THE System SHALL shrink the chip locally without a model request.
- **GT-8** WHEN the user types a character that differs from the chip's next character, THE System SHALL dismiss the chip and re-request.
- **GT-9** WHEN the app, field or caret line changes in a way that invalidates the anchor, THE System SHALL dismiss the chip.

### Acceptance
- **GT-10** WHILE a chip is visible, WHEN the user presses Tab, THE System SHALL insert the next word of the chip and keep the remainder visible, then re-request.
- **GT-11** WHILE a chip is visible, WHEN the user presses Shift+Tab, THE System SHALL insert the whole chip.
- **GT-12** WHILE a chip is visible, WHEN the user presses Esc, THE System SHALL dismiss the chip.
- **GT-13** WHILE no chip is visible, THE System SHALL NOT swallow any key event.
- **GT-14** THE System SHALL insert accepted text using synthetic Unicode key events, falling back to AX replacement and then clipboard paste with clipboard restore.

### Quality and personalization
- **GT-15** THE System SHALL cap chips at about 6 words and 48 characters and SHALL discard empty, duplicate, meta or prompt-echo outputs.
- **GT-16** THE System SHALL build prompts from a prefix-stable window so that consecutive requests share a prompt prefix.
- **GT-17** WHEN the user accepts text, THE System SHALL store it in a local, bounded, de-duplicated, most-recent-first memory and seed later prompts with a short summary of it.
- **GT-18** BEFORE sending any prompt, THE System SHALL redact credential-like tokens.

### Performance (targets, measured by the benchmark)
- **GT-19** WITH a warm local model on an M3/16 GB, THE System SHALL show the first chip within 150 ms p50 and 300 ms p95 of the last keystroke.
- **GT-20** Typing-through chip updates SHALL complete in under 16 ms.

## Out of scope
Screen capture/OCR context, multi-line suggestions, per-app profiles, Windows/Linux.

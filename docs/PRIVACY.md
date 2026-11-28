# Privacy

BOB is local-first. This document lists exactly what BOB reads, stores and sends. It is updated with every
feature that changes the answer.

## Summary
- No accounts, no telemetry, no analytics, no crash reporting.
- By default BOB reads only the field you type in, its window title and the text after your cursor (Accessibility).
  Reading the screen is opt-in (Personalize > What BOB can see): BOB then asks for Screen Recording, recognizes the
  text on this Mac and never stores or sends images.
- Text is sent only to the brain you configured. Local brains stay on `127.0.0.1`; cloud brains are labeled "cloud".
- Personal context (profile, tone, memory, window title, screen text) goes only to brains you allow, one switch per
  brain. Cloud brains start with it off.
- API keys are stored in the macOS Keychain only.

## Permissions (when each feature lands)
| Permission | Used for | If denied |
|---|---|---|
| Accessibility | read text near the caret, caret position, insert text, capture Tab/Esc only while a suggestion is visible | Ghost Text and Selection Actions are disabled with an explanation |
| Microphone | Voice Dictation | Dictation is disabled with an explanation |
| Screen Recording | only if you choose "Active window" or "All screens" as visible context | BOB stays at Accessibility-only context |
| Input Monitoring | only if the spike shows it is required for modifier-only hotkeys | documented in `docs/COMPATIBILITY.md` |

## Data handling
| Data | Where it goes | Stored? |
|---|---|---|
| Keys typed while a suggestion is shown | from the helper to BOB's core over the local socket, so Tab acts on what you really typed even when the app reports it late; like the text before the caret, never logged and never sent anywhere else | not stored |
| Text before the caret (Ghost Text) | the active Ghost brain, after redaction; a cloud brain gets nothing while the cursor is inside a word that may be the start of a key | not stored |
| Accepted phrases | local `ghost-memory.json` (at most 200 phrases of 64 characters, most recent first; secret-looking text is never kept); not sent to any brain yet | yes, locally; view and "Forget all" in the Ghost card |
| Quiet apps, Ghost on/off | local `ghost-settings.json` | yes, locally |
| Recently used apps (bundle id and name, last 12) | the quiet-apps list in the Ghost card | in memory only, gone when BOB quits |
| Your profile | local `profile.md` (0600), edited in Personalize; sent only to allowed brains, redacted | yes, locally |
| Tone, language, visible-context scope, Ghost layers | local `context-settings.json` | yes, locally |
| Window title, text after the cursor (up to 400 characters) | in memory; to allowed brains, redacted | no |
| Text recognized on screen (opt-in, up to 4,000 characters, 90 s lifetime) | in memory; to allowed brains, redacted. Screenshots stay in memory for the OCR call only | no |
| Selected text (Selection Actions) | read only when you press the hotkey; sent to the Selection brain. Cloud brains never receive text with credential-like tokens: the action is refused (redacting would write "[redacted]" into your document) | not stored |
| Explain and Reply | same as Selection Actions; the answer is shown in BOB's panel and goes to the clipboard only when you choose Copy | not stored |
| Clipboard (Selection Actions fallback) | borrowed for Cmd+C / Cmd+V when an app hides its selection from Accessibility, then restored item by item | not stored |
| Audio (Dictation) | recorded only while you hold the dictation key, kept in memory inside BOB's helper and transcribed there by whisper.cpp; a recording is at most 5 minutes | never written to disk, never sent |
| Transcripts | typed at the cursor; sent to the Dictation brain only when you pick a cleanup preset (and only with personal context if that brain is allowed); a transcript that looks like it contains a password or key is never sent to a cloud brain and is typed without cleanup | not stored, never logged |
| Last dictation (for "Type Last Dictation Again") | in memory only, forgotten after two minutes or when BOB quits | no |
| Voice edit | the selection is read with Accessibility when you start dictating over it; selection and spoken instruction go to the Selection brain, with the same cloud and secret rules as Selection Actions | not stored |
| Dictation vocabulary and replacements | local `dictation-settings.json`; the vocabulary is given to whisper on this Mac only | yes, locally |
| Usage statistics | local `usage.json`: per-day counts (words accepted, words dictated, rewrites, speech milliseconds), at most 400 days, never any text; Statistics > Clear | yes, locally |
| Running apps (quiet-apps picker) | bundle id and name, read when you open the list | not stored |
| Open at login | macOS Login Items, or `~/Library/LaunchAgents/dev.bob.assistant.login.plist` when macOS refuses the login item; removed when you turn it off | yes, by macOS |
| Typo fixes | the last word you finished is checked by the macOS spell checker inside BOB's helper | not stored |
| Text after the cursor (suggestions inside a sentence) | up to 160 characters of the same line, to the Ghost brain, redacted | not stored |
| Clipboard during a rewrite | BOB pastes the result, marks the clipboard item as transient (clipboard managers skip it) and restores your clipboard | not stored |
| Copied text (clipboard context, opt-in) | read only while the switch is on: plain text, at most 2,000 characters, kept in memory for two minutes; skipped for items marked private by password managers (nspasteboard.org concealed or transient types), copies in quiet apps and BOB's own copies; to allowed brains only, redacted | no |
| Per-app modes, preferred languages, style notes | local `context-settings.json` | yes, locally |
| Logs | `~/Library/Logs/BOB/bob.log` (0600, rotated at 5 MB, 3 old files): states, sizes and timings only; every line is also redacted | yes, locally |
| Activity trail | `~/Library/Logs/BOB/audit.jsonl` (0600, rotated at 2 MB): settings changes, brains (never keys), text sent to brains as feature, brain, cloud or not, character count and whether personal context was included; AI agent calls as tool, characters, duration and outcome | yes, locally |
| AI agents (MCP, opt-in) | text an agent sends goes to your brains with the same rules as the hotkeys; only same-user processes can connect | not stored (audited without text) |
| The "Try it" prompt (Brains panel) | to the brain you test; redacted for cloud brains; the activity trail records the size, never the text | not stored |
| Transcripts, typed text, selections | never logged | no |
| Separate profile (`BOB_HOME`, for testing; development builds only) | its settings, logs, keys and sockets stay under that folder and a separate Keychain service; only downloaded models and the window's theme are shared with your BOB | yes, in that folder |
| API keys | macOS Keychain | yes |

## Brains
- Every brain shows "on this Mac", "local" (loopback endpoint) or "cloud: text leaves your Mac". Unparseable endpoints
  count as cloud. Cloud brains are never used for Ghost Text unless a connection test proves they support it and you
  pick them.
- The built-in model server listens on `127.0.0.1` only, needs a random per-launch key (stored in a 0600 file in a
  0700 per-user directory, never on the command line) and has its web UI disabled. It is stopped when BOB quits; a
  leftover from a crash is stopped at the next launch.
- Model files are downloaded from Hugging Face only when you press Download, and are verified with SHA-256.

## Redaction
Before a prompt leaves the machine, credential-like tokens are dropped or replaced (`sk-`, `AKIA`, `ghp_`, JWTs,
PEM blocks, long opaque or hex tokens, long lowercase tokens that mix letters and digits such as UUIDs). A token
glued to other text by punctuation (`name,sk-...`, `a|ghp_...`, `f(...)`, quotes) is checked on its own.
Behavior is property-tested.

## Quiet zones
BOB does not read or suggest in secure text fields, when no caret position is known, or while text is selected.
Prompts are redacted before they leave the engine, even for the local brain.

Quiet apps get no suggestions and no text from them reaches any brain. The defaults are terminals (Terminal, iTerm, Warp,
Ghostty, kitty, Alacritty, WezTerm), password managers (1Password, Bitwarden, Passwords, Keychain Access) and BOB
itself; the Ghost card lists them with the apps you used recently, and
one switch per app adds or removes it. Changes apply at once, including to a suggestion already on screen.

## Screen context
- One frame per capture, of the window you type in or of every display, at most every 8 seconds unless you switch
  app or window. Never for secure fields, quiet apps or BOB's own windows.
- Text recognition is Apple's Vision framework on this Mac. Images are never written to disk or sent anywhere.
- macOS shows its screen-recording indicator while a frame is taken, and may ask you to confirm the permission again
  from time to time.
- "Preview what BOB sends" in Personalize shows exactly what would be added to prompts.

## Keyboard
While (and only while) a suggestion is visible, BOB swallows Tab, Shift+Tab and Esc. Shortcuts with Cmd, Ctrl, Option
or Fn always pass through. Text BOB types is marked so BOB never captures its own keystrokes. Key presses are not
logged or stored.

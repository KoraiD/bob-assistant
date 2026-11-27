# Changelog

All notable changes to BOB. The format follows [Keep a Changelog](https://keepachangelog.com/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]
### Security
- Release builds ignore the development environment variables (`BOB_HOME`, `BOB_NATIVE_PATH`,
  `BOB_LLAMA_SERVER`, `BOB_CONTROL_SOCKET`) and take `HOME`, `TMPDIR` and `PATH` from the system. Before, another
  program could start BOB with its own helper and have it run with BOB's Accessibility and Microphone permissions.
### Fixed
- Ghost Text: a suggestion that was dismissed with `Esc` while it was still arriving no longer comes back.
- Ghost Text: `⇧Tab` inserts what was on screen (finishing its last word), not words that arrived after the
  key press.
- Ghost Text: line breaks the model writes out as text (`\n`) end the suggestion instead of being inserted.
## [0.1.0] - 2026-11-26

The first release, built for Kiroween 2026.

### Ghost Text
- Suggestions at the caret in any app: `Tab` takes a word, `⇧Tab` the rest, `Esc` dismisses. Typing the suggested
  characters keeps it, without a new model call.
- Typo fixes from the macOS spell checker: after a misspelled word, Tab replaces it.
- Suggestions inside a sentence that fill the gap using the words after the cursor.
- Stays out of the way of an app's own completion: `@mentions`, `#channels`, `:emoji:` and `/commands`.
- Optional context: remembered phrases, the window title, text on screen, recently copied text and your languages.
- Suggestions in a language you do not write are dropped.
- Quiet apps (terminals and password managers by default), never in password fields.
- A per-app chip nudge for apps that report the cursor a little off.
- After the Mac wakes from sleep, BOB's own model and the speech model are warmed up again.
- Runaway model output (a character repeated eight or more times, such as "1000000000000") is never shown.
- Typing never waits for BOB: the key tap runs on its own thread and Accessibility calls to a busy app time out
  after 0.4 s.

### Selection Actions
- `⌘⇧J` on selected text: fix grammar, rephrase, shorten, friendlier, professional, summarize, translate, or your
  own actions. Paste-based replacement that `⌘Z` undoes in one step.
- Optional preview before replacing (`⏎` replaces, Copy keeps your text, `Esc` discards).
- Explain and Reply: the answer appears in the panel to copy or insert after the selection; nothing is replaced.
- A shortcut per custom action (`⌃⌥1` to `⌃⌥9`) runs it on the selection without the menu.
- Results are checked for the expected language, retried, and translated if needed.

### Voice Dictation
- Hold or tap `⌥Space`, speak, and the text appears at the cursor; whisper.cpp runs on the Mac.
- Voice edit: speak over a selection to change it ("make this friendlier").
- Vocabulary, text replacements, cleanup presets, start and stop sounds, the last dictation kept for two minutes,
  and an optional idle unload of the speech model.

### Brains and personalization
- BOB's own model (llama.cpp), Ollama, LM Studio or any OpenAI-compatible service; one brain per feature.
- A Markdown profile, tone, languages, US or UK spelling, quote style, style notes and per-app modes.
- Personal context goes only to brains you allow; cloud brains start with it off.

### App
- A Mac-style window with the haunted theme (Midnight, Daylight, Match macOS), a ghost that follows the mouse, and
  a live status orb.
- Welcome tour with permissions and a model recommendation for your Mac.
- Menu bar controls, pause (an hour, until tomorrow, until resumed), open at login, Dock icon option, ⌘1 to ⌘8.
- Statistics: words BOB typed for you and time saved (counts only).
- AI agents (MCP): `bob-mcp` and a Skill file for Kiro and other agents, off by default.
- Logs with five levels and an activity list of everything that matters for privacy, without your text.
- Signed and notarized release DMG with the model server and speech engine inside.

### Privacy and security
- No accounts and no telemetry. API keys only in the Keychain. Local servers bind to `127.0.0.1` with a
  per-launch key.
- Credential-like text is redacted from prompts and never sent to cloud brains by Selection Actions.
- Private (0600) settings files written atomically; local sockets accept only your own user; answers from a
  brain are size-limited; a crashed task cannot break the rest of the app.

### Fixed before release (found by tests and the owner's reports)
- In Chrome, a suggestion could repeat text already typed, because Chrome reports the caret before the text.
- A suggestion could stay frozen after switching windows in the same app.
- Ghost Text did not appear on the first line of a TextEdit document (the caret was reported one line too high).
- Selection Actions replacements were not undoable in some apps.
- Typing could freeze for a second or more and lose characters while an app was slow to answer Accessibility.
- Menu rows in the action panel were drawn at the top of each row instead of centered.
- Screen text could leak into rewrites.
- A Vietnamese suggestion appeared after Hungarian text.
- The Settings view could freeze the window (a lock taken twice).

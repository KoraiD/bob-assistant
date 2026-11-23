# Personal context: design
## Components
- **Swift helper**
  - `AXReader.snapshot` adds the focused window title (`AXFocusedWindow` > `AXTitle`) and `TextWindow.afterCaret`.
  - `HelperRuntime` sends `context_state` only when the title or after-caret text changes (not per keystroke).
  - `ScreenContext` (PC-4): `CapturePolicy` decides when (pure, tested). `SCShareableContent` and
    `SCScreenshotManager.captureImage` take one frame of the frontmost window (`CGWindowList` order picks it) or of
    each display (BOB and quiet apps excluded through `SCContentFilter`). Vision `VNRecognizeTextRequest`
    (accurate, language detection) runs off the main thread. `ScreenTextLayout.join` orders the lines and caps the
    text at 4,000 characters. Results for a scope the user has since turned off are dropped.
- **Protocol v3**
  - Events: `context_state`, `screen_text`, `permissions.screen_recording`.
  - Commands: `set_context_mode { scope, exclude }`, `request_screen_recording`.
  - The core pushes the mode after every handshake and whenever the scope or the quiet list changes.
- **Rust `context` module**
  - `ContextHub` holds `ContextSettings` (scope, tone, language, Ghost layers; `context-settings.json`), the profile
    (`profile.md`, 0600, atomic save) and `Visible` (app, title, after-caret text, latest screen text with a 90 s
    lifetime; active-window text only counts for the app it came from).
  - `ghost_preamble(memory)` and `chat_system(task, share)` assemble the layers (PC-5) and redact them.
- **Brains**
  - `BrainConfig.share_context: Option<bool>`; `shares_context()` defaults to `!is_cloud()` (PC-6).
  - The Ghost engine adds the preamble only for allowed brains, and so will the chat features.
- **UI**
  - A Personalize tab: the profile editor (toolbar, `react-markdown` + `remark-gfm` + `remark-emoji`, links and images
    neutralized), the visible-context scope with Screen Recording status, tone and language, Ghost layers, and a
    preview.
  - Brains tab: one switch per brain for sharing context.
## Ghost prompt shape
```
[Notes, not part of the text]
About me: Name: Dávid; PhD student and software engineer; ...
Phrases I often use: "as soon as possible", ...
Writing in Mail: Re: Project update
On screen: ...
[Text]
<the window of text before the caret>
```
Static layers come first so llama.cpp's prompt cache keeps covering them. The bracket labels keep a raw-continuation
model from continuing the notes.
## Correctness properties
- **P-PC1** The Ghost preamble stays within its character budget for any profile, title, screen text and memory
  (property test).
- **P-PC2** `condense` is bounded and never panics (property test).
- **P-PC3** Without permission (`shares_context() == false`) the prompt equals the typed-text window exactly (engine
  test).
- **P-PC4** Secrets in any layer never reach a prompt (unit test).
- **P-PC5** Capture never happens in Accessibility-only mode, for secure fields, for excluded apps, or more often than
  the interval while the window stays the same (Swift tests).

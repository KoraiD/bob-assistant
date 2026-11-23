# Personal context: requirements
Makes BOB's suggestions and rewrites fit the user: a profile file, writing-style settings, accepted-phrase memory
and what is visible around the caret. Owner decisions (2026-11-23) are recorded inline. EARS IDs `PC-n`.
## User stories
- As a writer, I want BOB to know my name, role, sign-off and favorite words so its suggestions sound like me.
- As someone replying to an email or chatting about an article on another screen, I want BOB to optionally see
  that text so its suggestions fit the conversation.
- As a privacy-conscious user, I want the default to read nothing beyond the field I type in, and I want to decide
  per brain whether my personal context may leave the Mac.
## Requirements
### Profile and settings
- **PC-1** THE System SHALL keep the user's profile as Markdown in a fixed file, `profile.md` in BOB's config
  directory, readable and editable with any editor. (Owner: fixed location, edited in the UI.)
- **PC-2** THE System SHALL provide an in-app editor for the profile with a formatting toolbar, a rendered
  GitHub-flavored preview and emoji (Unicode and `:shortcode:`), and SHALL refuse saves larger than 32 KB.
  The preview SHALL NOT follow links or fetch images.
### Visible context
- **PC-3** BY DEFAULT, THE System SHALL use only Accessibility for visible context: the focused window's title and
  up to 400 characters after the caret. It SHALL NOT read either for secure fields. (Owner: Accessibility by default.)
- **PC-4** WHERE the user opts in, THE System SHALL read the text visible in the active window, or on all
  displays, by capturing a single frame and recognizing text on the device. It SHALL request the Screen
  Recording permission only when the user picks such a scope, SHALL never capture BOB itself or quiet apps,
  SHALL capture at most once per 8 seconds unless the app or window changes, and SHALL never store images.
  (Owner: broader access must be possible for documents, email threads and other screens.)
- **PC-5** THE System SHALL assemble context layers in a fixed order, most stable first: BOB's instructions, tone and
  language, profile, memory, window, screen text, typed text. Every layer SHALL be redacted (GT-18 rules).
### Who receives it
- **PC-6** THE System SHALL send personal context (profile, tone and language, memory, window title, screen text) to
  a brain only when that brain is allowed to receive it. The default SHALL be allowed for brains on this Mac and not
  allowed for cloud brains; the user can change it per brain. (Owner: per brain, off by default for cloud.)
- **PC-7** WHILE an app is on the quiet list, THE System SHALL NOT capture it for screen context.
### Ghost Text
- **PC-8** Ghost Text SHALL use no context layer by default. Each layer (profile, memory, window, screen) SHALL be a
  separate switch, and the benchmark SHALL report each layer's effect on latency and accuracy before any default
  changes. (Owner: measure memory first, then decide; Ghost Text context may be removed.)
- **PC-8a** After the benchmark (owner, 2026-11-23): Ghost Text SHALL offer only remembered phrases (off by default),
  and the window title only as an addition to remembered phrases. Settings that ask for anything else SHALL be
  reduced to that rule.
### Transparency
- **PC-9** THE System SHALL let the user preview, on request, exactly what context would be added to Ghost Text and
  chat prompts, after redaction.
- **PC-10** THE System SHALL NOT log any context text (profile, titles, after-caret text, screen text).
## Out of scope
Cloud sync of the profile; images or screenshots sent to brains; per-app profiles; automatic profile learning.

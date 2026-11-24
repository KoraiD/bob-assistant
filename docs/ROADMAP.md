# What to build next (updated 2026-11-24, after Task 12a)

Deadline: **Fri Dec 4, 2026, 2:00 PM PT** (10 days left). Sources for the proposals: [RESEARCH.md](RESEARCH.md).
"Task 12a" is the feature and hardening batch built on Nov 24; packaging is now **Task 12b**.

## 1. Proposals: where each one stands

| # | Proposal | Status |
|---|---|---|
| 1 | Voice edit of a selection | Done (Task 11) |
| 2 | Vocabulary and text replacements | Done (Task 11) |
| 3 | Tap to toggle dictation | Done (Task 11) |
| 4 | Recover the last dictation | Done (Task 11) |
| 5 | Per-app modes (tone, language, instructions; Ghost on or off) | Done (Task 12a) |
| 6 | Preview before replace (Enter replaces, Copy, Esc discards) | Done (Task 12a), off by default |
| 7 | A shortcut per custom action | **Open.** Needs a second hotkey slot per action in the helper; about 1 day |
| 8 | Model recommendation for your Mac | Done (Task 11) |
| 9 | Report or nudge a misplaced suggestion per app | **Open.** Optional; the Chrome stale-caret fix (below) removes the most common cause |
| 10 | Quotes and US/UK spelling | Done (Task 11); spelling is now also converted by BOB because the model ignored it (Task 12a) |
| 11 | Unload the speech model when idle | Done (Task 11) |
| 12 | Start and stop sounds | Done (Task 12a), switch in the Dictation card |
| 13 | Usage statistics | Done (Task 11) |
| 14 | Meeting and audio-file transcription | **Out of scope** (a different product) |
| 15 | iCloud sync | **Out of scope** (privacy and scope) |
| 16 | Clipboard as context | Done (Task 12a), opt-in |
| 17 | MCP server and a Skill for agents | Done (Task 12a): [MCP.md](MCP.md), [skills/bob/SKILL.md](../skills/bob/SKILL.md) |

Also done in Task 12a: preferred writing languages and style notes (measured), security and reliability hardening,
multi-level logging and an audit trail, a Mac-style UI pass, a 192-case Ghost Text settings matrix and randomized
sessions, and fixes for the two Ghost Text bugs from the owner's screenshots.

## 2. Still open (not blocking the submission)

| Item | Why it is open | Size |
|---|---|---|
| Owner checks on real apps: voice edit (TextEdit, Mail), tap-to-toggle feel, vocabulary, Ghost Text in Chrome after the stale-caret fix, sounds, clipboard context, preview | Needs a person, a microphone and real apps | 30 min |
| Screen context in Mail, Safari and a second display (`personal-context` task 10) | Manual | 15 min |
| Cmd+Q on the build Mac (Vorssaint Quit Protection setting) | Owner setting, not a BOB bug | 2 min |
| Language filter limits: letters cannot separate Hungarian from German ("ü" is in both); a raw model drift is still possible, it is just not shown | Inherent to a letter and common-word filter; a language-ID model would be heavier | none planned |
| Proposals 7 and 9 | See the table above | 1 day each |
| Windows, Linux, Intel Macs | Out of scope (product steering: Apple Silicon, macOS 14+) | none |

## 3. Is Task 12b (packaging) needed for Kiroween?

**Yes.** The Devpost page asks for a "URL to your working app", and the Official Rules (summarized in
[SUBMISSION.md](SUBMISSION.md)) require the project to be installable and to run consistently on its platform, with
free access for judges until Jan 29, 2027. Today a judge can only build from source: the app bundle built here does
not contain the model server (`llama-server` is found at the development path `src-tauri/binaries/llama`), and
nothing is notarized, so a downloaded copy would be blocked by Gatekeeper.

The signing certificate on the build Mac is a **Developer ID Application** certificate, so a notarized release is
possible. What is missing is the notarization credential, passed through environment variables only.

### Task 12b steps

1. **Bundle the model server.** Copy `llama-server` and its ggml/llama dylibs into the bundle next to the executable
   (the lookup in `brain/sidecar.rs` already checks there), keep `@rpath` working, and sign each file.
2. **Release build.** `tauri build` in release mode (no `--debug`), arm64, with `bob-native`, `bob-mcp`,
   `whisper.framework` and the llama files inside. One script: `scripts/build-release.sh`.
3. **Sign for distribution.** Developer ID Application, hardened runtime, secure timestamp (`--timestamp`, not
   `--timestamp=none`), inside-out (dylibs, frameworks, helpers, then the app). Entitlements: microphone only.
4. **Notarize and staple.** `xcrun notarytool submit --wait` with `APPLE_ID`, `APPLE_PASSWORD` (app-specific) and
   `APPLE_TEAM_ID`, or an App Store Connect API key; then `xcrun stapler staple` on the app and the DMG.
5. **DMG.** A drag-to-Applications DMG with BOB's own artwork, its SHA-256 next to it.
6. **Clean-account test.** A new macOS user: open the DMG, first run, permission prompts, model downloads, all
   three features, MCP, quit. Record the result in COMPATIBILITY.md (`packaging-release` task 8).
7. **GitHub Release** `v0.1.0` with the DMG, checksum and notes; README "Install" section first, "Build from
   source" second.
8. **Fallback if notarization fails:** a Developer ID-signed, not notarized DMG, plus exact instructions. Since
   macOS 15, Control-click > Open no longer bypasses Gatekeeper; users go to System Settings > Privacy & Security >
   Open Anyway. Build-from-source instructions stay in the README either way.

Estimate: half a day with credentials, plus Apple's notarization wait (usually minutes).

## 4. Everything else needed to submit

| Item | Owner | Status |
|---|---|---|
| Public GitHub repo, MIT license detected in About, `/.kiro` at the root | done | Verified via the GitHub API (license `mit`) |
| URL to the working app (Task 12b release) | agent, with the owner's credentials | Open |
| Demo video under 3 minutes, public on YouTube, on the build Mac, no copyrighted music or third-party trademarks (prefer Apple's own apps on screen, and BOB's own sounds) | owner (script and shot list can be prepared) | Open |
| Devpost text description (features and functionality) | agent drafts, owner submits | Open |
| "How Kiro was used" write-up ([KIRO.md](KIRO.md)): finish the MCP, powers and IDE/CLI/Web/Crew sections | agent drafts, owner reviews | Partly done |
| Category **Frankenstein** and bonus choices in the form; student details (LinkedIn, student email, year and degree, school) for Best Student Project | owner | Open |
| Ownership confirmation (no employer or university claim) | owner | Open question in SUBMISSION.md |
| Kiro credits request (**by Tue Dec 1, 12:00 PM PT**) | owner | Open |
| Optional: blog post (Bonus Blog Prize) and `#KiroweenHackathon26` social post (Social Blitz, must be the owner's own words) | owner | Open |
| Keep the repo and release public and free until **Jan 29, 2027** | owner | Ongoing |

Suggested order: Task 12b release (needs credentials), then the owner checks, then the video on the released build,
then the write-ups and the form by Dec 3, leaving Dec 4 as a buffer.

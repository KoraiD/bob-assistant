# Roadmap (updated 2026-11-25)

Deadline: **Fri Dec 4, 2026, 2:00 PM PT**. What to submit and how: [SUBMISSION.md](SUBMISSION.md).

## Done

All proposals from [RESEARCH.md](RESEARCH.md) except the ones below, plus typo fixes, suggestions inside a sentence,
screen text for Ghost Text, a signed release DMG ([RELEASE.md](RELEASE.md)) and an end-to-end check (`make e2e`).
Details: [CHANGELOG.md](../CHANGELOG.md).

## Before submitting

| Item | Who | Notes |
|---|---|---|
| Choose the signing certificate for the public release | owner | The current one shows a company name; see SUBMISSION.md section 4 |
| Publish the GitHub Release (v0.1.0, DMG and checksum) | owner, or agent on request | `make release`, then a draft release |
| Owner checks on real apps | owner | List below |
| Demo video | owner | Shot list in SUBMISSION.md section 2 |
| Devpost form, student details, ownership | owner | SUBMISSION.md sections 3 and 4 |
| Notarization (optional, recommended) | owner | `xcrun notarytool store-credentials`, then `BOB_NOTARY_PROFILE=... make release` |
| README screenshots or a GIF | agent, after the video | From the release build |

**Owner checks** (each takes a minute or two):
1. Chrome and Gmail: type, pause, Tab; type through a suggestion; switch between two Chrome windows.
2. Slack: `@` plus a few letters, then Tab picks Slack's name. Type `/` commands and `:emoji:`.
3. Click into the middle of a sentence in Notes and wait for a suggestion.
4. Type "recieve " and press Tab.
5. Personalize > What BOB can see > Active window, then turn on "Use the text on screen"; reply to an email.
6. Selection Actions in Mail and Notes, then ⌘Z.
7. Voice edit and sounds in TextEdit.
8. Install the DMG on a second user account (System Settings > Users & Groups) and run the welcome tour.

## Possible additions (if time allows)

| Idea | Value | Size |
|---|---|---|
| Per-app chip offset nudge (proposal 9) | fixes rare misplaced suggestions in odd apps | half a day |
| A shortcut per custom action (proposal 7) | faster favorite rewrites | 1 day |
| "Explain" and "Reply" actions on a selection | common requests in the research | 2 hours |
| Model warm-up after wake from sleep | first suggestion after sleep is slower | 2 hours |
| README GIFs and a landing section | presentation score | 1 hour |

## Out of scope

Meeting and audio-file transcription, iCloud sync, Windows, Linux and Intel Macs.

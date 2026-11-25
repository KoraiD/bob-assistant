# Kiroween 2026 submission guide

Deadline: **Friday, Dec 4, 2026, 2:00 PM Pacific** (23:00 in Budapest). No late submissions. Sources: the Devpost
overview page (read Nov 24) and the Official Rules (read Nov 22). Re-read the Rules tab before submitting; they can
change.

## 1. Checklist

| # | Item | Status |
|---|---|---|
| 1 | Public GitHub repository with all source, assets and instructions | Done: https://github.com/KoraiD/bob-assistant |
| 2 | Open source license detected by GitHub (About section) | Done: MIT |
| 3 | `/.kiro` at the root, never in `.gitignore` | Done; checked by a hook and by CI |
| 4 | URL to the working app | **Open**: publish the GitHub Release with the DMG ([RELEASE.md](RELEASE.md)) |
| 5 | Text description of features and functionality | Draft below (section 5) |
| 6 | Demo video, under 3 minutes, public on YouTube | **Open**: guide in section 2 |
| 7 | Write-up of how Kiro was used | Done: [KIRO.md](KIRO.md). Add CLI, Web or Crew use if any |
| 8 | Category (Frankenstein) and bonus prizes chosen in the form | **Open**: section 3 |
| 9 | Student details for Best Student Project | **Open**: section 3 |
| 10 | Ownership confirmed (no employer or university claim) | **Open**: section 4 |
| 11 | Kiro credits request (optional) | Request by **Tue Dec 1, 12:00 PM PT** if you need them |
| 12 | Keep the repository and the release public and free until **Jan 29, 2027** | Ongoing |

Rules that shape the submission:
- **New project only:** the repository history starts on Nov 22, 2026, and only standard libraries are used, listed
  in [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md).
- **Insufficient use of Kiro can disqualify an entry.** Stage 1 judging is pass/fail on the theme and on the
  required tools.
- **The project must install and run on its platform** and match the video and the description.
- **Judges may use AI tools to analyze the repository,** so the README and docs must explain themselves.
- **All materials in English.**

## 2. The demo video

Goal: under 3:00, BOB working end to end on the Mac it was built on, problem first and features second.

**Before you record**
- Use the release build from the DMG, not a development build.
- Turn on a Focus mode (no notifications), quit Kiro and anything personal, and hide desktop icons.
- Use a clean user profile in BOB: Personalize > profile with sample data, Ghost memory cleared or a sample set,
  and no real names, emails or work content anywhere on screen.
- Use Apple's own apps (TextEdit, Notes, Mail with a demo account), not Slack, Chrome or Office. The rules forbid
  third-party trademarks and copyrighted material unless you have permission.
- No music, or only music you made yourself. Your own voice-over works best.
- Record with Screenshot (⌘⇧5, "Record Selected Portion") at 1080p or higher, with the microphone on for the
  dictation part.

**Shot list (about 2:45)**

| Time | Scene | What to show |
|---|---|---|
| 0:00 to 0:15 | The problem | "AI writing help usually means copying text into a chat window, and sending it to someone else's server." |
| 0:15 to 0:30 | BOB | The app with the ghost (move the mouse so its eyes follow it). "BOB brings that help to every app, with the model you choose, on your Mac." |
| 0:30 to 1:05 | Ghost Text | In Notes, type a sentence slowly; the suggestion appears. Tab, Tab, then ⇧Tab. Type a typo ("recieve") and fix it with Tab. Click into the middle of a sentence and get a gap suggestion. |
| 1:05 to 1:35 | Selection Actions | Select a rough paragraph, press ⌘⇧J, choose Make professional, then ⌘Z to show the undo. Optionally Translate. |
| 1:35 to 2:05 | Voice Dictation | Hold ⌥Space and say a sentence; then select text, hold ⌥Space and say "make this friendlier". |
| 2:05 to 2:25 | Bring your own brain | Brains: the local model, Ollama or a cloud model with its "cloud" label, and the per-brain context switch. Personalize: languages and writing style. |
| 2:25 to 2:40 | Privacy and agents | Settings > Activity (what was sent, sizes only), then AI agents: Kiro calling `bob_rewrite`. |
| 2:40 to 2:50 | Close | "BOB: Bring Your Own Brain. Built with Kiro." The repository URL. |

**After recording:** trim to under 3:00, upload to YouTube as **Public** (not Unlisted), title it
"BOB: Bring Your Own Brain (Kiroween 2026)", and put the repository link in the description.

## 3. The Devpost form

1. Start the submission on the hackathon page ("Enter a Submission"). You can save drafts.
2. **Category:** Frankenstein. **Bonus prizes:** Best Student Project (Frankenstein); Best Demo needs no extra step.
   Add Social Blitz or the Blog Prize only if you post.
3. **Links:** the repository, the release page (the "working app" URL), and the YouTube video.
4. **Student details** go into the questions the form shows when you select Best Student Project. Those answers
   are visible to the organizers and judges only, not on the public project page. The rules ask for:
   - your LinkedIn profile;
   - your student email address (at the school's domain);
   - your year and degree (for example "MSc, 2nd year");
   - your school's name.
   Keep proof of enrollment ready (a student ID or an enrollment certificate). Devpost may ask for it, and not
   answering in time disqualifies the entry from that prize.
5. If the form has a field for testing instructions for judges, add: "Download the DMG from Releases, open
   System Settings > Privacy & Security > Open Anyway if macOS asks, and follow the welcome tour."

## 4. Ownership: no employer or university claim

The rules require the submission to be your original work and solely owned by you, with no other person or entity
having any right to it. There is no separate upload for this: submitting the form is your confirmation. Before you
submit, check:
- Your **employment contract** and any IP or invention-assignment policy. If it covers work done outside working
  hours or on your own equipment, ask your employer for a short written statement that they claim no rights to BOB.
- Your **university's IP rules**. Coursework and personal projects are usually yours, but projects that used
  university funding or lab equipment may not be.
- **The signing certificate.** The Developer ID on the build Mac belongs to a company, and its name is visible to
  anyone who downloads the signed DMG. If you enter as an individual (required for the student prize), either sign
  the release with a personal Developer ID, or get the company's written confirmation that it claims no rights.
  The safest option is your own Apple Developer account.

Keep any written confirmation; Devpost can ask for it during verification.

## 5. Description draft (for the form)

> **BOB (Bring Your Own Brain)** is a privacy-first writing assistant for macOS that works in every app you type in.
> **Ghost Text** suggests the next words at your cursor, fixes typos and fills gaps inside a sentence.
> **Selection Actions** rewrite, shorten, translate or polish selected text in place. **Voice Dictation** turns
> speech into text, or into an edit of the selected text. It runs on a local model on your Mac by default
> (llama.cpp and whisper.cpp), or on Ollama, LM Studio or any OpenAI-compatible service you choose. Cloud models are
> clearly labeled and get no personal context unless you allow it.
>
> It is a Frankenstein of technologies stitched into one app: a Rust core (Tauri 2), a Swift helper using macOS
> Accessibility, event taps and native overlays, llama.cpp and whisper.cpp, a React UI, and an MCP server so AI
> agents like Kiro can use it too. It was built spec-first with Kiro: eight EARS specs, seven steering files, seven
> hooks, two custom agents, MCP and property-based tests. Every quality decision was measured on the real model.

## 6. Optional extras

- **Blog post** (Bonus Blog Prize): a template is in [blog-post-template.md](blog-post-template.md). It must be
  published on a third-party platform during the submission period and say it was written for Kiroween 2026.
- **Social post** (Social Blitz): must start with `#KiroweenHackathon26`, be written by you, and say how Kiro changed
  the way you build. Judged on original thoughts, so write it yourself.

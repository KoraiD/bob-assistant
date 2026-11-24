# Kiroween 2026 submission checklist
The current plan, open items and the packaging steps are in [ROADMAP.md](ROADMAP.md).

Sources: the Devpost overview page and the **Official Rules** (page updated Nov 16, 2026; read 2026-11-22).
Rule section numbers below refer to the Official Rules. Re-read the Rules tab before submitting: they can change.

## Key facts
- Registration: Oct 13 to **Dec 4, 2026, 2:00 PM PT**. Submission period: Oct 30, 9:00 AM PT to **Dec 4, 2:00 PM PT**. No extensions, no late submissions.
- Judging: Dec 16, 2026 to Jan 29, 2027. Winners announced around Feb 3, 2027.
- Sponsor: Amazon Web Services. Administrator: Devpost.
- Teams up to 3 (this project: solo). Category: **Frankenstein**.
- One eligible project can win **one** Overall or Category prize **plus any number** of Bonus prizes (section 8).

## Hard rules that shape the build
| Rule | What it means for BOB |
|---|---|
| New projects only; built during the Submission Period; **disclose any pre-existing code** (sec. 4) | Fresh repo, history starts Nov 22. Only standard frameworks/libraries. The README lists third-party components. |
| Insufficient use of Kiro = disqualification (sec. 4); Stage 1 is pass/fail on "fits the theme and applies the required tools" (sec. 6) | Keep real Kiro usage visible: specs, steering, hooks, agents, MCP, PBT, in git history and in the write-up. |
| Public GitHub repo, license file detectable in the About section, `/.kiro` at the root and never in `.gitignore` | Enforced by `scripts/check-kiro.sh`, a hook and CI. |
| The project must be installable and run consistently on its intended platform, and match the video/description | macOS 14+ Apple Silicon is the declared platform. Everything shown in the video must work from the shipped build. |
| Free, unrestricted access for judges **until the Judging Period ends (Jan 29, 2027)** | Keep the repo public and the release downloadable and free through Jan 29, 2027. No paid-only dependencies. |
| Third-party SDKs/APIs/data: you must be authorized under their terms (sec. 4) | Record every dependency and model license in `THIRD_PARTY_NOTICES.md` (Task 10). **Verify model licenses before shipping** (see below). |
| No third-party trademarks or copyrighted music in the video | Original artwork only; no music or royalty-free with a license we can cite. |
| Submission must be solely owned by the Entrant, with no other person or entity having any right or interest (sec. 4) | Owner confirms no employer or university IP claim (see open questions). |
| Project must not have had financial/preferential support from the Sponsor or Administrator (sec. 4) | Standard Kiro credits for the hackathon are fine; nothing else. |
| Judges may use automated AI-driven analysis (sec. 6) | The README, specs and docs must be clear and self-explanatory. |
| English submission materials | All docs and video in English. |

## Required deliverables
- [ ] Public GitHub repo (GitHub only) with visible license in About
  - [x] MIT `LICENSE` at the root
  - [x] Repo created under the owner's personal GitHub account, pushed, public: https://github.com/KoraiD/bob-assistant
  - [x] License visible in the About section (GitHub API reports `mit`, 2026-11-24)
- [x] `/.kiro` at the root, never ignored (specs, steering, hooks, agents)
  - [x] `.kiro/settings/mcp.json` (added by the owner: the agent is blocked from writing that folder)
- [ ] URL to the functional app: a GitHub Release with the `.dmg`, plus build-from-source instructions
- [ ] Text description of features and functionality
- [ ] Demo video: **under 3 minutes**, public on YouTube, shows the project running on the build device, no third-party trademarks or copyrighted material
- [ ] "How Kiro was used" write-up (`docs/KIRO.md`): IDE/CLI/Web/Crew differences, custom agents, hooks, specs vs vibe coding, steering, MCP, PBT, powers
- [ ] Identify the category (Frankenstein) and the bonus categories in the form

## Judging criteria (equal weight, 20% each)
Potential Impact, Technological Implementation (Kiro depth, strategy, experimentation), Design, Innovation/Idea, Presentation.

## Bonus prizes: exact requirements from the rules
| Prize | Exact requirement | Plan |
|---|---|---|
| **Best Demo** ($1k) | Awarded to the highest **Presentation** score that is **also in the top 10% of overall scores**. It is not free of conditions: the whole project must score well. | Strong video, but never at the expense of product quality. |
| **Best Student Project: Frankenstein** ($1.5k + 1-year Kiro Pro Max) | Indicate the category; confirm you are attending classes at a recognised academic institution; provide **LinkedIn profile, student email, year and degree, school name**. Proof of current enrollment may be requested; if it is not provided in time you can be disqualified from this prize. | Owner provides details in the Devpost form; keep proof of enrollment ready. |
| **Bonus Blog Prize** (2,000 Kiro credits, top 100 scoring submissions with a post) | Public post about **Kiro and your project build** on a third-party platform, published during the Submission Period, **must mention it is written as part of the Kiroween 2026 hackathon**; URL in the form. | Owner publishes by ~Dec 1; draft kept in `docs/` once written. |
| **Social Blitz** (3-month Kiro Pro+, 5 winners) | Post on any social platform that **starts with `#KiroweenHackathon26`**, shares your favorite thing about Kiro and **how it changed the way you approached development**; posted during the period; link in the form. The Kiro team judges **original thoughts vs AI writing**, insights and post assets. The social profile must match your other submission details. | Owner writes it personally (not AI-written); use a screenshot or short clip from the real build. |
| Startup / Ghost of Kiroween Past / Kiro Crew App | Not targeted (not confirmed / different product). | none |

## Kiro credits (time-sensitive)
Each entrant may receive a one-time allotment of **4,000 Kiro credits**. Request via the Credit Request Form with a **Kiro user ID or AWS account ID**.
**Request by Tue Dec 1, 2026, 12:00 PM PT. Redeem by Fri Dec 4, 2026. Redeemed credits expire Dec 31, 2026.** Allow 48 business hours for review.
- [ ] Owner submits the credit request form (earlier is better).

## Open questions for the owner
1. **Ownership:** the rules require the submission to be solely owned by the Entrant with no other person or entity having any right. Please confirm that neither your full-time employer nor your university has any claim on this project, and decide whether you enter as an individual or as your company (an Organization is an eligible Entrant). The student bonus requires an individual student.
2. ~~Model licenses~~ resolved in Task 3: every catalog model is Apache-2.0 (Qwen2.5-1.5B-Instruct, Qwen3-4B-Instruct-2507, Qwen3-1.7B-Base); Qwen2.5-3B was dropped (non-Apache). Listed in `THIRD_PARTY_NOTICES.md`.
3. **Signing:** release builds may be signed and notarized with the owner's company Developer ID. Notarization needs App Store Connect API credentials supplied through environment variables; they must never be committed.

## Repo hygiene before submitting
- [ ] `make check` passes
- [ ] No secrets, models or binaries tracked
- [ ] README screenshots/GIFs, architecture, Kiro summary, `THIRD_PARTY_NOTICES.md`
- [ ] No private identifiers (work accounts, internal names) in files or commit history
- [ ] Commit history starts on/after Oct 30, 2026
- [ ] Release stays downloadable until Jan 29, 2027

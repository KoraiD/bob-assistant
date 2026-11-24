# Research: similar tools and what their users ask for (2026-11-24)

Summarized from product pages, reviews and user forums. All wording here is paraphrased. Links are the sources.

## The landscape
| Category | Tools | What they do | Gap BOB fills |
|---|---|---|---|
| Inline autocomplete | [Cotypist](https://cotypist.app/), [TypeSeer](https://news.ycombinator.com/item?id=49760583), open-source clones ([KeyType](https://github.com/johnbean393/KeyType), [Pretype](https://github.com/nikiomori/Pretype), [TabType](https://github.com/nilava/TabType), [GhostType](https://github.com/mk668a/GhostType)) | grey suggestions at the cursor, Tab to accept, local models | Cotypist does not rewrite selections or dictate ([comparison](https://auramacapps.hashnode.dev/cotypist-vs-draftkey-vs-wundertype-vs-rewritebar-local-ai-writing-on-mac-compared)); only local models, no "bring your own brain" |
| Rewrite selection | WunderType, RewriteBar, Apple Writing Tools, PopClip, Raycast AI | select, shortcut, rewrite in place | Apple Writing Tools depend on app support ([RewriteBar article](https://rewritebar.com/articles/mac-grammar-check)); most tools do one gesture only |
| Dictation | [Superwhisper](https://superwhisper.com/), Wispr Flow, [VoiceInk](https://github.com/Beingpax/VoiceInk), MacWhisper | push to talk, local or cloud speech, AI cleanup | Wispr Flow is cloud-first; MacWhisper is file-first ([Superwhisper vs MacWhisper](https://superwhisper.com/vs/macwhisper)) |
| All three in one | DraftKey (autocomplete + rewrite, no cloud option) | | BOB: all three gestures, any brain, local by default |

## What users and reviewers value
- **On-device privacy is the main reason to choose a tool.** Reviewers single out Cotypist for running fully local
  ([HyperWrite review](https://www.hyperwriteai.com/articles/cotypist-review)). Wispr Flow drew criticism for
  capturing screenshots of the active window for context without making it clear, and for keeping dictation data
  unless users opt out ([summary](https://iconpolls.com/blogs/wispr-flow-review-2026-ai-voice-dictation-app-login-pricing-and-user-experience-and-faqs),
  [Voibe](https://www.getvoibe.com/resources/wispr-flow-linkedin-dictation-analysis/)).
  BOB's opt-in screen context and "Preview what BOB sends" answer this directly.
- **Feeling native on the Mac.** Daring Fireball praised Cotypist for following macOS conventions, down to where it
  stores data and models ([link](https://daringfireball.net/linked/2026/06/18/cotypist)).
- **A guided first run** with permissions, model recommendations for your Mac and examples
  ([review](https://chrischinchilla.com/blog/2025/cotypist-ai-autocomplete-macos-review/)).
- **Turning it off quickly:** for this app, globally, for a while, or indefinitely, for example while recording the
  screen (same review). *Done in Task 10 (pause and quiet apps).*

## What users ask for (open requests and gaps)
- **Custom vocabulary and text replacements** for names and jargon. Superwhisper includes them even on the free plan
  ([pricing](https://superwhisper.com/docs/billing/plans)); VoiceInk has a personal dictionary and "Smart Replace"
  ([summary](https://voiceink.software.informer.com/)).
- **Modes per app or task with their own shortcuts.** Superwhisper users request a hotkey per mode
  ([request](https://feedback.superwhisper.com/feedback/p/custom-key-shortcuts-for-different-modes-one-step-process),
  [another](https://feedback.superwhisper.com/feedback/p/set-default-model-after-switch-to-another-model)). Cotypist
  offers per-app instructions in its top tier. VoiceInk has a per-app "Power Mode".
- **Pausing a recording** mid-dictation
  ([request](https://feedback.superwhisper.com/feedback/p/pause-recording)).
- **Sync** of modes, vocabulary and replacements across devices
  ([request](https://feedback.superwhisper.com/feedback/p/synchronize-across-devices)).
- **Hold or tap:** Superwhisper treats a hold as push-to-talk and a quick tap as start/stop
  ([settings](https://superwhisper.com/docs/get-started/essential-settings)).
- **Recovering a transcript** when the text did not land: "recovery built in" is a selling point of
  [Sayburst](https://alternativeto.net/software/sayburst/about/).
- **Reviewing a rewrite before it replaces the text** (DraftKey) and **diffs** (RewriteBar)
  ([comparison](https://auramacapps.hashnode.dev/cotypist-vs-draftkey-vs-wundertype-vs-rewritebar-local-ai-writing-on-mac-compared)).
- **Voice commands on a selection:** VoiceInk can rewrite selected text from a spoken command
  ([listing](https://tooldirectory.ai/tools/voiceink)).
- **Misplaced suggestions:** the TypeSeer author says positioning was the hardest part, and added a way for users
  to report and correct misalignment per app ([Show HN](https://news.ycombinator.com/item?id=49760583)).
- **Language and typography preferences:** UK vs US English and straight vs curly quotes appear among Cotypist's
  personalization settings ([review](https://chrischinchilla.com/blog/2025/cotypist-ai-autocomplete-macos-review/)).

## Proposals
The proposal table and the status of each one are in [ROADMAP.md](ROADMAP.md).

*Content was rephrased for compliance with licensing restrictions.*

# How Kiro was used

BOB was built from an empty repository between Nov 22 and Dec 4, 2026, with Kiro as the main development tool. This
page answers the Devpost questions: which Kiro features were used, how, and what each one changed. The day-by-day
record is in [kiro-journal.md](kiro-journal.md).

## Kiro surfaces

The **Kiro IDE**, in agent mode, for planning, specs, code, tests, debugging and documentation. The agent ran the
builds, the test suites, the benchmarks, the app itself and end-to-end checks on the Mac, and read the logs it
produced. _Owner: add any use of the Kiro CLI, Web or Crew here, and how they compared._

## Spec-driven development

Eight specs in `.kiro/specs/`, each with `requirements.md` (EARS, with IDs such as GT-7 or SA-6), `design.md`
(components, sequence diagrams and correctness properties) and `tasks.md` (each task linked to requirement IDs):
`native-bridge`, `brain-router`, `ghost-text`, `personal-context`, `selection-actions`, `voice-dictation`,
`haunted-ui` and `packaging-release`.

What it changed:
- **Tests came from the requirements.** Each requirement ID became a named test, so coverage gaps were visible.
- **Hard parts were designed before they were coded.** The personal-context spec was written from the owner's
  answers before any code, and settled what BOB may read, what it may send and to which models.
- **Vibe coding for the small stuff, specs for the core.** UI polish and one-off scripts were done
  conversationally. The engines, the protocol and anything touching privacy went through a spec first.

## Steering

Seven files in `.kiro/steering/`:
- Always on: `product`, `tech`, `structure`, `perf-budgets` and `privacy-security`. These hold the
  non-negotiables: latency budgets, no telemetry, keys only in the Keychain, and logs without user text.
- Loaded only for matching files: `haunted-theme` for UI files and `macos-native` for Swift files.

What it changed: the rules held across hundreds of turns without repeating them. For example, the rule "measure,
research, change one thing, re-measure" in `perf-budgets` is why every performance and quality decision in
[PERFORMANCE.md](PERFORMANCE.md) comes with numbers. `macos-native` collected the lessons learned the hard way,
for example the Chromium caret APIs and permission grants tied to the signing identity, so they were applied in
later sessions.

## Agent hooks

| Hook | When | What it does |
|---|---|---|
| `rust-fmt-on-save` | a `.rs` file is saved | formats Rust |
| `swift-build-on-save` | a `.swift` file is saved | builds the helper, so compile errors show at once |
| `ts-typecheck-on-save` | a `.ts`/`.tsx` file is saved | type-checks the UI |
| `kiroween-guard` | `.gitignore`, `.kiro/` or `LICENSE` is saved | blocks anything that would ignore `.kiro` or remove the license |
| `stop-quick-check` | end of every agent turn | the same Kiroween rule check |
| `stop-bench-smoke` | end of every agent turn | replays a typing trace through the real Ghost engine (about 3 s) |
| `task-docs-reminder` | after a spec task | reminds the agent to update docs, `tasks.md` and the journal |

What it changed: format, build and type errors never piled up, and the Kiroween rules (`.kiro` committed and never
ignored, MIT license present) were checked on every turn.

## Custom agents

`.kiro/agents/` has two agents with their own instructions and steering:
- `macos-native-reviewer`: a read-only reviewer for Accessibility, event-tap and overlay code.
- `perf-auditor`: enforces the measure-first process for latency work.

## MCP

- **Kiro used MCP while building BOB:** the `fetch` server for documentation and research (llama.cpp server flags,
  MCP protocol revisions, the hackathon rules, competitor research in [RESEARCH.md](RESEARCH.md)).
- **BOB is an MCP server:** `bob-mcp` lets Kiro and other agents use BOB's models and writing style (rewrite,
  suggest, status, pause), off by default and audited. A Skill file, [skills/bob/SKILL.md](../skills/bob/SKILL.md),
  tells other tools when and how to use it. `.kiro/settings/mcp.json` has a `bob` entry for the development build.
  See [MCP.md](MCP.md).

## Property-based testing

About 25 properties in Rust (`proptest`), taken from the "correctness properties" sections of the designs: the wire
protocol, prompt windows, chip polishing, redaction, downloads (only verified files are installed), the dictation
state machine, the language filter and more. Swift uses seeded randomized tests.

What it found: on its first run, the protocol property "decoding does not depend on how the bytes are split" failed
on a caret rectangle. `serde_json` parses about 18% of such floats one unit off by default. Shrinking reduced it to
one message, and the fix was a single feature flag. An example-based test with round numbers would have passed.

## Measurement as part of the loop

Kiro ran the real model in benchmarks (`make bench`, `make style-bench`) and the real app in end-to-end checks
(`make e2e`), and changed code only when the numbers said so. Examples, all recorded in
[PERFORMANCE.md](PERFORMANCE.md):
- Remembered phrases raised the right-next-word rate from 34% to 46%, and screen text to 53%.
- The model ignored the US/UK spelling instruction entirely, so BOB now converts the spelling itself.
- A worked-example "gap" prompt fixed suggestions inside a sentence, where plain continuation produced nonsense.
- The end-to-end run found bugs no unit test could: no caret on TextEdit's first line, rewrites that were not
  undoable, and screen text leaking into rewrites.

## Powers

No Kiro powers were used. None of those available matched this stack (Tauri, Rust, Swift on macOS), so the stack
knowledge lives in steering instead.

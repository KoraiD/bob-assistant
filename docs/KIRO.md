# How Kiro was used

This is the living draft of the Devpost "how Kiro was used" write-up. It is fed by `docs/kiro-journal.md`.
Judges score depth and variety of Kiro usage, so each section states what we did and what it changed.

## Kiro surfaces
IDE (specs, steering, hooks, PBT). CLI/Web/Crew: _record any use and the differences here_.

## Specs (`.kiro/specs/`)
Seven specs, each with `requirements.md` (EARS), `design.md` (components, sequence diagrams, correctness
properties) and `tasks.md` (traceable to requirement IDs):
`brain-router`, `native-bridge`, `ghost-text`, `selection-actions`, `voice-dictation`, `haunted-ui`, `packaging-release`.
Spec-driven vs vibe coding observations: see the journal.

## Steering (`.kiro/steering/`)
`product`, `tech`, `structure` (foundation); `perf-budgets` and `privacy-security` (always on: the project's
non-negotiables); `haunted-theme` (loaded for UI files); `macos-native` (loaded for Swift files).
Custom agents list steering explicitly in `resources` because steering is not auto-included for them.

## Hooks (`.kiro/hooks/`)
| Hook | Trigger | Purpose |
|---|---|---|
| rust-fmt-on-save | PostFileSave `\.rs$` | formatting |
| swift-build-on-save | PostFileSave `\.swift$` | catch compile errors immediately |
| ts-typecheck-on-save | PostFileSave `\.(ts|tsx)$` | catch type errors immediately |
| kiroween-guard | PostFileSave `.gitignore`, `.kiro/`, `LICENSE` | enforce Kiroween rules |
| stop-quick-check | Stop | Kiroween rule guard each turn |
| stop-bench-smoke | Stop | oracle replay through the real Ghost engine (about 3 s): latency and scoring regressions surface every turn |
| task-docs-reminder | PostTaskExec (agent) | keep docs, tasks.md and journal in sync |

## Custom agents (`.kiro/agents/`)
`macos-native-reviewer` (read-only review of Swift/AX/event-tap code) and `perf-auditor` (enforces
measure, research, change one thing, re-measure).

## MCP
Planned: `fetch` server for documentation lookups during the build, and a custom `bob-bench` server exposing
benchmark results to Kiro (stretch). The workspace file `.kiro/settings/mcp.json` was created by the owner because the
agent's permission rules block writes to that folder (a deliberate safeguard). Content:

```json
{
  "mcpServers": {
    "fetch": { "command": "uvx", "args": ["mcp-server-fetch"], "disabled": false }
  }
}
```

## Property-based testing
Rust `proptest` for pure logic (see the "Correctness properties" sections in each design.md). Kiro's PBT
(IDE-only) derives properties from the EARS requirements; results are recorded in the journal.

## Powers
_To be evaluated from the Kiro powers registry (Tauri/Rust/macOS/testing). Record which were used and why._

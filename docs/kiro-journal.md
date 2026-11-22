# Kiro journal

Running log feeding `docs/KIRO.md` and the Devpost write-up. One entry per task or notable event:
what worked, what did not, IDE vs CLI observations, spec vs vibe-coding comparisons.

## 2026-11-22: Task 1, foundation
- **Planning before code:** researched the hackathon page and the Kiro docs (specs, hooks, steering, PBT, powers,
  custom agents, MCP) first, then wrote the seven specs with EARS requirement IDs that tests reference.
- **Steering split:** always-on files hold non-negotiables (performance budgets, privacy). File-matched files hold
  domain detail (haunted theme for UI files, macOS notes for Swift files), which keeps context small.
- **Hooks:** created through the hook tool (v1 JSON). Save hooks give immediate feedback on Rust, Swift and TS.
  A guard hook protects the Kiroween rule that `.kiro` must never be ignored.
- **Friction:** the agent could not write `.kiro/settings/mcp.json` (permission rule on the settings folder). The
  owner has to create it. Also, writing JSON files with a remote `$schema` URL was refused in supervised mode, so
  the Tauri configs omit `$schema` (it is optional).
- **Tooling surprise:** Vitest 2 pulled a nested Vite and broke `vite.config.ts` typing; fixed by moving to Vitest 3.
- **Tests at this point:** Rust 4 (incl. 2 property tests), Swift 2, Vitest 3.

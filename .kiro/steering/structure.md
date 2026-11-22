---
inclusion: always
---

# Project structure

```
.kiro/            Kiro artifacts: specs/, steering/, hooks/, agents/, settings/mcp.json  (never ignore!)
src/              React/TS UI (components/, lib/, styles/)
src-tauri/        Rust core (src/, capabilities/, icons/, tauri.conf.json)
native/bob-native Swift helper (Sources/BobNativeCore, Sources/bob-native, Tests/)
scripts/          check-kiro.sh, gen-icon.py, later: fetch-models, build-sidecars, bench
docs/             ARCHITECTURE, PRIVACY, PERFORMANCE, KIRO, COMPATIBILITY, DESIGN, SUBMISSION, kiro-journal
assets/           Source artwork (bob-icon.png)
.github/          CI workflows
```

- Rust modules are flat under `src-tauri/src/` and named by responsibility
  (`app_info`, later `native_ipc`, `brain`, `ghost`, `selection`, `dictation`, `settings`).
- Feature work follows its spec in `.kiro/specs/<feature>/` (requirements.md, design.md, tasks.md).
- Never write secrets, models (`*.gguf`, `*.bin`) or sidecar binaries into the repo.

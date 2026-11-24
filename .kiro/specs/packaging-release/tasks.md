# Packaging and release: tasks

Plan tasks 10 and 12 (repo hygiene).

- [x] 1. MIT LICENSE, `.gitignore` that never ignores `.kiro`, `scripts/check-kiro.sh` guard (_PR-6, PR-9_) (Task 1)
- [x] 2. CI workflow for Rust, Swift, UI and Kiro guard (_PR-7_) (Task 1; audit job added in Task 10)
- [ ] 3. `scripts/versions.env` + `scripts/build-sidecars.sh` (pinned, Metal, arm64) (_PR-2, PR-3_)
- [ ] 4. `externalBin` config and scoped shell capabilities (_PR-2_)
- [x] 5. Bundle the Swift helper; signing identity strategy (_PR-4_): `scripts/build-app.sh` (helper, whisper.framework, bob-mcp); identity from the environment
- [ ] 5.1 Bundle llama-server and its dylibs (Task 12b, see docs/ROADMAP.md)
- [x] 6. First-run model download (_PR-5_): welcome tour with the recommendation for this Mac (Task 11)
- [ ] 7. DMG build, notarization or documented ad-hoc path (_PR-4_)
- [ ] 8. Clean-account install test; record in docs (_PR-1_)
- [ ] 9. README quickstart, screenshots/GIFs, THIRD_PARTY_NOTICES.md (_PR-3, PR-8_)

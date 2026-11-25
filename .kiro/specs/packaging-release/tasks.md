# Packaging and release: tasks

Plan tasks 10 and 12 (repo hygiene).

- [x] 1. MIT LICENSE, `.gitignore` that never ignores `.kiro`, `scripts/check-kiro.sh` guard (_PR-6, PR-9_) (Task 1)
- [x] 2. CI workflow for Rust, Swift, UI and Kiro guard (_PR-7_) (Task 1; audit job added in Task 10)
- [x] 3. `scripts/versions.env` and pinned, SHA-256-verified downloads of llama.cpp and whisper.cpp (_PR-2, PR-3_)
- [x] 4. The model server ships in `Contents/Resources/llama` and is started by the Rust core; no shell capability (_PR-2_)
- [x] 5. Bundle the Swift helper; signing identity strategy (_PR-4_): `scripts/build-app.sh` (helper, whisper.framework, bob-mcp); identity from the environment
- [x] 5.1 Bundle llama-server and its dylibs (`scripts/build-release.sh`)
- [x] 6. First-run model download (_PR-5_): welcome tour with the recommendation for this Mac (Task 11)
- [x] 7. Signed DMG; notarization ready behind `BOB_NOTARY_PROFILE`, not run yet (_PR-4_, docs/RELEASE.md)
- [ ] 8. Clean-account install test; record in docs (_PR-1_)
- [x] 9. README install and build steps, THIRD_PARTY_NOTICES.md (_PR-3, PR-8_)
  - [ ] 9.1 Screenshots or a GIF in the README (after the demo recording)

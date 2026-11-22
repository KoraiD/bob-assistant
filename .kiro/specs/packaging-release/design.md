# Packaging and release: design

- `scripts/build-sidecars.sh`: clone pinned llama.cpp / whisper.cpp tags, build with Metal for arm64, copy to
  `src-tauri/binaries/<name>-aarch64-apple-darwin`, ad-hoc or Developer ID sign them. Pins live in `scripts/versions.env`.
- `tauri.conf.json > bundle.externalBin` lists the sidecars; `capabilities` scope shell execute to exact binaries.
- The Swift helper is built in release mode and bundled as an additional resource/binary, signed with the same identity so TCC attributes permissions consistently.
- Signing: dev builds use a stable local identity; releases use Developer ID + notarization (App Store Connect API key via env vars) if the owner provides credentials, otherwise ad-hoc (`signingIdentity: "-"`) with documented Gatekeeper steps.
- CI (`.github/workflows/ci.yml`): macOS runner; jobs for Rust, Swift, UI, Kiro guard.
- Clean-account test: create a fresh macOS user, install the DMG, complete onboarding, run the three capabilities.

## Properties / checks
- `scripts/check-kiro.sh` runs in CI and as a hook; fails when `.kiro` is missing/ignored, LICENSE missing or secrets are present.
- A `release-check` script verifies sidecar versions match `versions.env` and that no model/binary files are tracked.

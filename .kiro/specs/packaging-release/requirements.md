# Packaging and release: requirements

Something a judge can run. EARS IDs `PR-n`.

## Requirements
- **PR-1** THE repository SHALL contain all source, assets and instructions needed to build and run BOB from a clean macOS 14+ Apple Silicon machine.
- **PR-2** THE System SHALL be buildable with documented commands (`make setup && make build`) and a script that fetches/builds pinned sidecars.
- **PR-3** THE build SHALL pin llama.cpp and whisper.cpp versions and record their licenses in `THIRD_PARTY_NOTICES.md`.
- **PR-4** THE release bundle SHALL contain a signed `.app` and `.dmg` (Developer ID + notarization if available, ad-hoc otherwise with instructions for Gatekeeper).
- **PR-5** WHEN first launched without models, THE System SHALL download the default models with progress and verification.
- **PR-6** THE repository SHALL display a detectable MIT license and SHALL include the `/.kiro` directory, never ignored.
- **PR-7** CI SHALL run Rust tests and clippy, Swift tests and the TypeScript checks on every push.
- **PR-8** THE README SHALL include a judge-friendly quickstart, screenshots/GIFs, architecture overview and Kiro usage summary.
- **PR-9** THE repository SHALL NOT contain secrets, models or sidecar binaries.

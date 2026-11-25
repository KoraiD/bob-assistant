# Releasing BOB

`make release` builds `release/BOB-<version>-arm64.dmg` and its `.sha256`. The `release/` folder is ignored by git.

## What the script does

1. Builds the release app (`tauri build`), the Swift helper and `bob-mcp`.
2. Puts everything inside `BOB.app`: the helper and `bob-mcp` in `Contents/MacOS`, `whisper.framework` in
   `Contents/Frameworks`, the llama.cpp server and its 10 libraries in `Contents/Resources/llama`, plus the licenses.
   Benchmark tools are not built (they need `--features bench`).
3. Signs inside out with the hardened runtime and a secure timestamp: libraries, the model server, the framework,
   the helper (microphone entitlement), `bob-mcp`, then the app.
4. Creates the DMG (drag to Applications) and signs it.
5. Notarizes and staples, **only** when `BOB_NOTARY_PROFILE` is set.

## Secrets

Nothing secret goes into the repository, the build log or an environment file.
- **Signing identity:** `BOB_SIGN_IDENTITY` names a certificate in your login keychain (its name or SHA-1). The
  private key never leaves the keychain. The script stops without it, unless `BOB_ALLOW_ADHOC=1` (local testing
  only).
- **Notarization credentials:** stored once in the keychain with
  `xcrun notarytool store-credentials <profile> --apple-id <id> --team-id <TEAMID>` (it asks for an app-specific
  password). The build only receives the profile name.
- CI never signs or notarizes; releases are built on a trusted Mac.

## Steps for a release

```bash
export BOB_SIGN_IDENTITY="Developer ID Application: <Name> (<TEAMID>)"
make check && make release            # add BOB_NOTARY_PROFILE=<profile> to notarize
```

Then check the result:

```bash
codesign --verify --strict --deep "/Volumes/BOB <version>/BOB.app"   # after opening the DMG
spctl -a -vvv -t exec "/Volumes/BOB <version>/BOB.app"                 # "Notarized Developer ID" once notarized
```

Publish on GitHub: a release named `v<version>` with the DMG, the `.sha256` file and the notes from CHANGELOG.md.
Start with a draft release, check the download on a second Mac or user account, then publish.

## Verified (2026-11-25, v0.1.0)

- The DMG is 25 MB and the app 58 MB; `codesign --verify --strict --deep` passes.
- Gatekeeper reports "Unnotarized Developer ID" (expected until notarization).
- Run from the mounted DMG: the bundled model server started from `Contents/Resources/llama`, the helper
  connected, and quitting stopped both.

## Without notarization

macOS blocks the first launch of a downloaded app that is not notarized. Users open System Settings > Privacy &
Security and click **Open Anyway** (since macOS 15, Control-click > Open no longer skips this). Notarize before a
wide release.

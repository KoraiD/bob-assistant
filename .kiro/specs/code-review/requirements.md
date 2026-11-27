# Code review: requirements

A full review of BOB before the 0.5.0 beta: security, performance, logging and tests, code quality and docs, plus a
way to test the whole app safely while the build machine is in use.

## Scope
- **CR-1:** Every tracked file is read once with six lenses: security (S), correctness (C), performance (P), logging,
  audit and privacy (L), tests (T), simplicity and style (Q). Lockfiles and images are checked by tools only.
- **CR-2:** Tools run over every file: `cargo audit`, `cargo deny`, `npm audit`, `semgrep` (Rust, TypeScript, React,
  secrets, GitHub Actions, Swift), `clippy -D warnings`, `tsc --noEmit`, `swiftlint`, coverage (`cargo llvm-cov`).
- **CR-3:** Flaws the owner saw (extra spaces, wrong word endings, at a human typing speed, in a plain editor and in an
  Electron editor) are reproduced before they are fixed.

## Safe testing
- **CR-4:** Tests never type into the owner's apps. Ghost Text, Selection Actions and Dictation are exercised by a
  simulator (the real engines on a virtual clock against a fake app, helper, model and user) and by the Test Bench
  (a BOB-owned editor that a test copy of BOB suggests into).
- **CR-5:** The test copy of BOB runs in a separate profile (`BOB_HOME`): its own settings, logs, sockets and
  Keychain service, so it never touches the owner's data. Bad values stop it instead of falling back.
- **CR-6:** The Test Bench types only into its own window, holds the owner's BOB instance lock while it runs, pauses
  when the mouse is used or another window comes to the front, and stops on any key press.

## Findings
- **CR-7:** Each finding has an ID, severity, location, evidence (a reproduction, a measurement or the code path),
  a proposed fix and the test that will prove it. Confirmed bugs get a failing regression test first.
- **CR-8:** Findings are kept in two places: full notes locally (not in the repository) and a cleaned list in this
  spec (`findings.md`), without personal data, signing identities or home-folder paths.

## Fixing
- **CR-9:** Nothing is fixed without the owner's approval of the batch it belongs to (exceptions approved explicitly:
  GT-13, GT-14, GT-15 during the Test Bench work, and DOC-01).
- **CR-10:** Every fix ships with its test, its doc update and a conventional commit naming the finding IDs. Bug fixes
  and optimizations follow the project's process: reproduce, measure, change one thing, re-measure, record.
- **CR-11:** After the fix batches: CI gains the missing gates, the docs are consolidated (no duplication, no stale
  claims), and BOB is released as 0.5.0 beta with an update check and shortcut remapping.

# Native bridge: tasks

Plan task 2.

- [x] 1. Protocol schema in Rust (serde) and Swift (Codable) with shared fixtures (_NB-3_)
  - [x] 1.1 Round-trip + chunking property tests P1, P2 (_NB-4_)
- [x] 2. Socket server in Rust, client in Swift, hello handshake (_NB-1, NB-2_)
  - [x] 2.1 Version mismatch test P3
- [x] 3. Helper supervision: spawn from core, restart with backoff, exit when core exits (_NB-5_)
- [x] 4. Permission detection and onboarding links (Accessibility, Microphone) (_NB-6_)
- [x] 5. AX focus/text/selection/caret observation via AXObserver (_NB-7_)
- [x] 6. Test chip at the caret in TextEdit and Notes
- [ ] 7. Compatibility matrix (procedure written; needs the owner to run `make probe` per app) in `docs/COMPATIBILITY.md` (TextEdit, Notes, Mail, Safari, Chrome, VS Code, Slack) (_NB-9_)
- [ ] 8. Resolve spike questions: TCC attribution of nested helper (needs the packaged app, Task 10); whisper-server availability (Task 8)

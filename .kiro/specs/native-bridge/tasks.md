# Native bridge: tasks

Plan task 2.

- [ ] 1. Protocol schema in Rust (serde) and Swift (Codable) with shared fixtures (_NB-3_)
  - [ ] 1.1 Round-trip + chunking property tests P1, P2 (_NB-4_)
- [ ] 2. Socket server in Rust, client in Swift, hello handshake (_NB-1, NB-2_)
  - [ ] 2.1 Version mismatch test P3
- [ ] 3. Helper supervision: spawn from core, restart with backoff, exit when core exits (_NB-5_)
- [ ] 4. Permission detection and onboarding links (Accessibility, Microphone) (_NB-6_)
- [ ] 5. AX focus/text/selection/caret observation via AXObserver (_NB-7_)
- [ ] 6. Test chip at the caret in TextEdit and Notes
- [ ] 7. Compatibility matrix in `docs/COMPATIBILITY.md` (TextEdit, Notes, Mail, Safari, Chrome, VS Code, Slack) (_NB-9_)
- [ ] 8. Resolve spike questions: TCC attribution of nested helper; whisper-server availability; report to user

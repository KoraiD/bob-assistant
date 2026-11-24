# Personal context: tasks
Tick a box in the same commit as the code.

- [x] 1. Protocol v3: `context_state`, `screen_text`, `set_context_mode`, `request_screen_recording`, `permissions.screen_recording`; shared fixtures (_PC-3, PC-4_)
- [x] 2. Helper: window title and after-caret text; send on change only (_PC-3_)
- [x] 3. Helper: `ScreenContext` with `CapturePolicy`, ScreenCaptureKit frame, Vision OCR, own and quiet apps excluded (_PC-4, PC-7_)
  - [x] 3.1 Swift tests: after-caret window, line ordering and cap, capture policy (P-PC5)
- [x] 4. Core: `context` module (settings, profile, visible state, assembly, redaction) with unit and property tests (_PC-1, PC-5, PC-10_; P-PC1, P-PC2, P-PC4)
- [x] 5. Brains: per-brain `share_context`, default by locality; Ghost engine uses the preamble only for allowed brains (_PC-6_; P-PC3)
- [x] 6. UI: Personalize tab (profile editor, scope, style, Ghost layers, preview) and the per-brain switch (_PC-2, PC-9_)
- [x] 7. Benchmark: `ghost-bench --context` with a sample profile, seeded memory and window titles; results in docs/PERFORMANCE.md (_PC-8_): memory is the only layer above noise (+8.5 points right word, +4 ms p50)
- [x] 8. Owner decision on Ghost Text defaults after the benchmark (_PC-8, PC-8a_): memory off by default, offered alone, window title only as an add-on; enforced by `ContextSettings::sanitized` and the Personalize card
- [x] 9. Selection Actions and Dictation use `chat_system` (Tasks 7 and 8)
- [ ] 10. Manual check: screen context in Mail, Safari and a second display; compatibility matrix
- [x] 11. Writing style: quote style applied to Ghost chips, dictation and rewrites; spelling preference in `chat_system` (Task 11)

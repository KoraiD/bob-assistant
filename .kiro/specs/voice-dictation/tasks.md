# Voice Dictation: tasks
Plan task 8. Tick a box in the same commit as the code.
- [x] 1. Pure state machine with property test P1 (_VD-3, VD-4, VD-11_): `dictation::machine`
- [x] 2. Decide and wire the resident whisper approach; model catalog entries (_VD-2_): official whisper.cpp xcframework in the helper (decision log); `SPEECH_CATALOG` with large-v3-turbo Q5_0 (default) and base
- [x] 3. Native: hotkey hold detection, AVAudioEngine capture to 16 kHz mono, permission flow (_VD-1, VD-7_): Carbon press/release (default Option+Space), `AudioRecorder`, microphone prompt on first use
- [x] 4. Transcription, VAD, golden test (_VD-2, VD-5_): `Transcriber`, energy-based `SpeechGate`, golden test with `say` audio generated at test time
- [x] 5. Cleanup presets via brain with fallback P2 (_VD-6_): Clean, Translate to English, Meeting notes; off by default
- [x] 6. Insert at caret; secure field guard (_VD-9_): in the engine and in the helper's insert path; spacing after a word
- [x] 7. Orb listening/thinking states (_VD-3_): native `OrbPanel` near the caret, level-reactive, reduced motion honored
- [x] 8. Latency measurement recorded in `docs/PERFORMANCE.md` (_VD-10_): model timings on fixtures; release-to-insert is shown in the card
  - [ ] 8.1 Owner check: real dictation in TextEdit, Mail, Chrome; release-to-insert numbers; an M3 measurement
- [x] 9. Docs and journal
## Task 11 additions (owner-chosen proposals, docs/RESEARCH.md)
- [x] 10. Voice edit over a selection: `dictation_selection` event, `SelectionHandle::voice_edit`, cleanup skipped (engine test)
- [x] 11. Vocabulary as whisper prompt and text replacements (unit tests for prompt bounds and whole-word replacement)
- [x] 12. Tap to toggle: `Latched` state, P1 extended to both modes
- [x] 13. Last dictation kept two minutes; menu bar and card
- [x] 14. Optional idle unload and reload on press
- [x] 15. Usage counts for Statistics
  - [ ] 15.1 Owner check: voice edit in TextEdit and Mail; tap-to-toggle feel; vocabulary on a name whisper usually misses
## Task 12a additions
- [x] 16. Start and stop sounds (proposal 12): synthesized in the helper, trimmed from the recording, switch in the card (Swift test)

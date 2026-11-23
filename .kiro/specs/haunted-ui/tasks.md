# Haunted UI: tasks
Plan task 9; tokens and the shell start in Task 1 and grow with each feature. Tick a box in the same commit as the code.
- [x] 1. Design tokens, ghost mark, app shell, reduced-motion CSS (_UI-2, UI-6_) (Task 1)
- [x] 2. Contrast property test over token pairs P1 (_UI-7_): `src/styles/tokens.test.ts`, 15 text pairs per theme, translucent layers composited; found 7 failing daylight pairs, fixed by darkening the daylight accent, pumpkin and status colors
- [x] 3. Sidebar navigation and panels (_UI-1_): Overview, Ghost Text, Selection Actions, Voice Dictation, Brains, Personalize, About
- [x] 4. Orb and state animations (_UI-3_): `activity.rs` derives idle/listening/thinking/haunting/error from engine commands (no engine changes) and emits `orb-state`; `Orb.tsx` in the sidebar; the native dictation orb shares the palette
- [x] 5. First-run wizard with permissions and model setup (_UI-4_): welcome, permissions, brain, voice, cheat sheet; skippable, reopened from About
- [x] 6. Tray states; hide-on-close (_UI-5_): tray tooltip follows the orb state; close hides the window (existing handler)
- [x] 7. Swift Palette synced with tokens, test P2 (_UI-10_): parsed from `ChipPanel.swift` in the token test
- [x] 8. Keyboard/focus/aria audit (_UI-8_): axe-core on every panel and the wizard in Vitest, visible `:focus-visible` ring; axe found one issue (aria-label on a span), fixed
- [x] 9. Docs/DESIGN.md and journal

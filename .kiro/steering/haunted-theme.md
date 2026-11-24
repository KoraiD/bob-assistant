---
inclusion: fileMatch
fileMatchPattern: ["src/**/*.tsx", "src/**/*.css", "native/**/*Overlay*.swift", "native/**/*Chip*.swift", "docs/DESIGN.md"]
---

# Haunted theme guide

BOB is a **friendly ghost**: spooky but warm, never gory or scary. Design counts toward judging
(Design, Presentation), so every screen should feel intentional and coherent.

## Tokens (single source: `src/styles/globals.css`)
- Surfaces: midnight indigo (`--bg #0c0b16`, `--surface #15132a`). Default theme is dark; "daylight" is optional.
- Primary: ectoplasm green `--accent #8affc1`. Secondary: pumpkin `--accent-2 #ff9f5a`.
- Mac-native structure, haunted palette (owner request, 2026-11-24): System Settings style groups and rows
  (`components/ui.tsx`: `Group`, `Row`, `Toggle`, `Segmented`, `Button`, `Icon`), `--radius-card 10px`, SF Pro for
  text and the rounded face only for BOB's name and big numbers. Line icons, not emoji, in navigation.
- The spooky part lives in the palette, the ghost (eyes follow the mouse, blush, bounce), the orb and the microcopy,
  not in decoration. Soft glows (`--glow`) and gentle floating motion only on the ghost.
- The Swift chip mirrors these colors; keep them in sync via `docs/DESIGN.md`.

## Rules
- Use CSS variables, never hard-coded hex values in components.
- Motion is slow and gentle (float, fade like ectoplasm). **Always honor `prefers-reduced-motion`.**
- Maintain readable contrast (WCAG AA) in both themes; check text on `--surface` and on glow.
- Microcopy: playful ghost voice ("Summoning BOB...", "BOB is listening...") but always clear first.
  Errors must say what happened and what to do next.
- Orb states: idle, listening, thinking, haunting (suggestion visible), error.
- No third-party trademarks, logos or copyrighted audio in the UI or demo.

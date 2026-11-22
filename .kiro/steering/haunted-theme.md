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
- Rounded shapes (`--radius-card 18px`), soft glows (`--glow`), subtle floating motion.
- The Swift chip mirrors these colors; keep them in sync via `docs/DESIGN.md`.

## Rules
- Use CSS variables, never hard-coded hex values in components.
- Motion is slow and gentle (float, fade like ectoplasm). **Always honor `prefers-reduced-motion`.**
- Maintain readable contrast (WCAG AA) in both themes; check text on `--surface` and on glow.
- Microcopy: playful ghost voice ("Summoning BOB...", "BOB is listening...") but always clear first.
  Errors must say what happened and what to do next.
- Orb states: idle, listening, thinking, haunting (suggestion visible), error.
- No third-party trademarks, logos or copyrighted audio in the UI or demo.

# Haunted UI: design

Tokens live in `src/styles/globals.css`; the rationale and palette table live in `docs/DESIGN.md`.

- Layout: left sidebar navigation, soft cards, ghost mark in the header, orb floating window (Task 9).
- Native chip/menu use the same colors via a generated Swift `Palette` (kept in sync by a test that parses the CSS tokens).
- State model: the Rust core emits `orb_state` events; React renders from them (no business logic in the UI).
- Accessibility: tests assert accessible names, reduced-motion CSS and contrast ratios computed from token values.

## Correctness properties
- **P1 (UI-7):** for every foreground/background token pair used for text, contrast ratio >= 4.5 (computed in a Vitest test).
- **P2 (UI-10):** every color in the Swift `Palette` equals its CSS token (parsed in a test).

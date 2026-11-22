/**
 * BOB's face: a friendly ghost. Pure SVG so it scales crisply and inherits the
 * theme through CSS variables (`--accent`, `--accent-fg`).
 */
export function GhostMark({ size = 64, float = false }: { size?: number; float?: boolean }) {
  return (
    <svg
      role="img"
      aria-label="BOB the friendly ghost"
      width={size}
      height={size}
      viewBox="0 0 64 64"
      className={float ? "ghost-float" : undefined}
    >
      <path
        d="M32 6c-11.6 0-20 8.6-20 20v28c0 1.9 2.2 2.9 3.6 1.6l4.2-3.9a2.6 2.6 0 0 1 3.5 0l3.7 3.4c1 .9 2.5.9 3.5 0l3.5-3.2a2.6 2.6 0 0 1 3.4 0l3.5 3.2c1 .9 2.5.9 3.5 0l3.7-3.4a2.6 2.6 0 0 1 3.5 0l4.2 3.9c1.4 1.3 3.6.3 3.6-1.6V26C52 14.6 43.6 6 32 6z"
        fill="var(--accent)"
      />
      <circle cx="24" cy="28" r="4" fill="var(--accent-fg)" />
      <circle cx="40" cy="28" r="4" fill="var(--accent-fg)" />
      <path
        d="M26 38c1.8 2.4 4 3.4 6 3.4s4.2-1 6-3.4"
        fill="none"
        stroke="var(--accent-fg)"
        strokeWidth="2.6"
        strokeLinecap="round"
      />
    </svg>
  );
}

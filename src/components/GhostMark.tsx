import { useEffect, useRef, useState } from "react";

/** What BOB says when clicked, in order. */
export const GHOST_LINES = [
  "Boo!",
  "Hi, I'm BOB.",
  "Your words stay on this Mac.",
  "Tab takes a word, Shift+Tab takes them all.",
  "I don't bite. I type.",
  "Happy Kiroween!",
];

/** Largest pupil offset in SVG units (the eye radius is 4.6, the pupil 2.5). */
const MAX_LOOK = 1.9;

/** Pupil offset towards the pointer at (`dx`, `dy`) pixels from the ghost's center. */
export function lookOffset(dx: number, dy: number): { x: number; y: number } {
  const dist = Math.hypot(dx, dy);
  if (dist < 1) return { x: 0, y: 0 };
  const reach = Math.min(MAX_LOOK, dist / 40);
  return { x: (dx / dist) * reach, y: (dy / dist) * reach };
}

const prefersReducedMotion = () => typeof window !== "undefined" && !!window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;

const BODY =
  "M32 6c-11.6 0-20 8.6-20 20v28c0 1.9 2.2 2.9 3.6 1.6l4.2-3.9a2.6 2.6 0 0 1 3.5 0l3.7 3.4c1 .9 2.5.9 3.5 0l3.5-3.2a2.6 2.6 0 0 1 3.4 0l3.5 3.2c1 .9 2.5.9 3.5 0l3.7-3.4a2.6 2.6 0 0 1 3.5 0l4.2 3.9c1.4 1.3 3.6.3 3.6-1.6V26C52 14.6 43.6 6 32 6z";

/**
 * BOB's face: a friendly ghost. Pure SVG so it scales crisply and inherits the theme through CSS variables.
 * `interactive`: the eyes follow the pointer, hovering makes BOB blush and wiggle, and a click makes him
 * bounce and say something. All motion stops when the OS asks for reduced motion (the eyes stay centered).
 */
export function GhostMark({ size = 64, float = false, interactive = false }: { size?: number; float?: boolean; interactive?: boolean }) {
  const ref = useRef<SVGSVGElement>(null);
  const [look, setLook] = useState({ x: 0, y: 0 });
  const [hover, setHover] = useState(false);
  const [bounce, setBounce] = useState(false);
  const [line, setLine] = useState<number | null>(null);
  const [blink, setBlink] = useState(false);
  const clicks = useRef(0);

  useEffect(() => {
    if (!interactive || prefersReducedMotion()) return;
    let frame = 0;
    let last: MouseEvent | null = null;
    const update = () => {
      frame = 0;
      const el = ref.current;
      if (!el || !last) return;
      const r = el.getBoundingClientRect();
      setLook(lookOffset(last.clientX - (r.left + r.width / 2), last.clientY - (r.top + r.height * 0.44)));
    };
    const onMove = (e: MouseEvent) => {
      last = e;
      if (!frame) frame = requestAnimationFrame(update);
    };
    window.addEventListener("mousemove", onMove, { passive: true });
    // An occasional blink, every 3 to 6 seconds.
    let blinkTimer: ReturnType<typeof setTimeout>;
    const scheduleBlink = () => {
      blinkTimer = setTimeout(() => {
        setBlink(true);
        setTimeout(() => setBlink(false), 140);
        scheduleBlink();
      }, 3000 + Math.random() * 3000);
    };
    scheduleBlink();
    return () => {
      window.removeEventListener("mousemove", onMove);
      if (frame) cancelAnimationFrame(frame);
      clearTimeout(blinkTimer);
    };
  }, [interactive]);

  useEffect(() => {
    if (line === null) return;
    const t = setTimeout(() => setLine(null), 2600);
    return () => clearTimeout(t);
  }, [line]);

  const eyes = (
    <g className={blink ? "ghost-blink" : undefined}>
      {[24, 40].map((cx) =>
        interactive ? (
          <g key={cx}>
            <circle cx={cx} cy="28" r="4.6" fill="#ffffff" />
            <circle cx={cx + look.x} cy={28 + look.y} r="2.5" fill="var(--accent-fg)" data-testid="ghost-pupil" />
          </g>
        ) : (
          <circle key={cx} cx={cx} cy="28" r="4" fill="var(--accent-fg)" />
        ),
      )}
    </g>
  );

  const svg = (
    <svg
      ref={ref}
      role={interactive ? undefined : "img"}
      aria-label={interactive ? undefined : "BOB the friendly ghost"}
      aria-hidden={interactive ? true : undefined}
      width={size}
      height={size}
      viewBox="0 0 64 64"
      className={[float && !bounce ? "ghost-float" : "", hover ? "ghost-wiggle" : "", bounce ? "ghost-bounce" : ""].filter(Boolean).join(" ") || undefined}
      onAnimationEnd={(e) => {
        if (e.animationName === "ghost-bounce") setBounce(false);
      }}
    >
      <path d={BODY} fill="var(--accent)" />
      {eyes}
      {interactive && (
        <g className="ghost-blush" style={{ opacity: hover ? 0.55 : 0 }} aria-hidden>
          <ellipse cx="19" cy="35" rx="3.2" ry="2" fill="#ff7aa8" />
          <ellipse cx="45" cy="35" rx="3.2" ry="2" fill="#ff7aa8" />
        </g>
      )}
      {interactive && (bounce || line !== null) ? (
        <ellipse cx="32" cy="39.5" rx="3" ry="3.4" fill="var(--accent-fg)" />
      ) : (
        <path d="M26 38c1.8 2.4 4 3.4 6 3.4s4.2-1 6-3.4" fill="none" stroke="var(--accent-fg)" strokeWidth="2.6" strokeLinecap="round" />
      )}
    </svg>
  );

  if (!interactive) return svg;
  return (
    <span className="relative inline-grid place-items-center">
      <button
        type="button"
        aria-label="BOB the friendly ghost. Click to say hello."
        onPointerEnter={() => setHover(true)}
        onPointerLeave={() => setHover(false)}
        onFocus={() => setHover(true)}
        onBlur={() => setHover(false)}
        onClick={() => {
          setLine(clicks.current % GHOST_LINES.length);
          clicks.current += 1;
          if (!prefersReducedMotion()) setBounce(true);
        }}
        className="ghost-button cursor-pointer rounded-full"
      >
        {svg}
      </button>
      <span aria-live="polite" className="ghost-bubble-anchor">
        {line !== null && <span className="ghost-bubble">{GHOST_LINES[line]}</span>}
      </span>
    </span>
  );
}

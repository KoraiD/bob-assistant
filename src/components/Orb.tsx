import { orbLabel, type OrbView } from "../lib/orb";
import { GhostMark } from "./GhostMark";

/**
 * The BOB orb (UI-3): the ghost inside a ring that changes with BOB's state. Idle floats, listening pulses,
 * thinking spins, haunting glows, error turns pumpkin. The label is announced politely to screen readers.
 */
export function Orb({ view, size = 56 }: { view: OrbView; size?: number }) {
  return (
    <div className="flex items-center gap-3" data-testid="orb" data-state={view.state}>
      <div className={`orb orb-${view.state}`} style={{ width: size + 16, height: size + 16 }}>
        <span className="orb-ring" aria-hidden="true" />
        <GhostMark size={size} float={view.state === "idle" || view.state === "haunting"} interactive />
      </div>
      <p role="status" aria-live="polite" className="text-xs" style={{ color: view.state === "error" ? "var(--accent-2)" : "var(--text-muted)" }}>
        {orbLabel(view)}
      </p>
    </div>
  );
}

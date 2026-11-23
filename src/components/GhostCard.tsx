import { useCallback, useEffect, useState } from "react";
import { isTauri } from "../lib/appInfo";
import { api, ghostSummary, type GhostStatus, type Snapshot } from "../lib/brains";
import { useTauriEvent } from "../lib/useTauriEvent";

/** Overview card for Ghost Text: on/off and live latency (refreshes every 2 s while visible). */
export function GhostCard() {
  const [status, setStatus] = useState<GhostStatus | null>(null);
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);

  const refresh = useCallback(async () => {
    if (!isTauri()) return;
    try {
      const [g, s] = await Promise.all([api.ghostStatus(), api.snapshot()]);
      setStatus(g);
      setSnapshot(s);
    } catch {
      /* the next tick retries */
    }
  }, []);

  useEffect(() => {
    void refresh();
    const t = setInterval(() => void refresh(), 2000);
    return () => clearInterval(t);
  }, [refresh]);
  useTauriEvent("brains-changed", () => void refresh());

  const toggle = async () => {
    if (!status) return;
    await api.ghostSetEnabled(!status.enabled);
    void refresh();
  };

  const summary = isTauri()
    ? ghostSummary(status, snapshot?.roles.ghost ?? null, snapshot?.builtin.state === "ready")
    : "Open the BOB app to use Ghost Text.";
  const s = status?.stats;
  return (
    <section aria-label="Ghost Text" className="soft-card panel-in mt-6 p-5">
      <div className="flex items-center justify-between gap-4">
        <div>
          <h2 className="text-lg font-medium">Ghost Text</h2>
          <p className="text-sm" style={{ color: "var(--text-muted)" }} data-testid="ghost-summary">
            {summary}
          </p>
        </div>
        {status && (
          <button
            type="button"
            role="switch"
            aria-checked={status.enabled}
            aria-label="Ghost Text"
            onClick={toggle}
            className="rounded-full px-4 py-1.5 text-sm font-medium"
            style={status.enabled ? { background: "var(--accent)", color: "var(--accent-fg)" } : { background: "var(--surface-raised)", color: "var(--text-muted)", border: "1px solid var(--border-strong)" }}
          >
            {status.enabled ? "On" : "Off"}
          </button>
        )}
      </div>
      {s && s.chips_shown > 0 && (
        <p className="mt-3 text-xs" style={{ color: "var(--text-muted)" }} data-testid="ghost-stats">
          {s.chips_shown} suggestions · {s.accepted_words} words accepted with Tab · {s.accepted_all} with Shift+Tab · {s.typing_through} typed through · {s.dismissed} dismissed
        </p>
      )}
      {s?.last_error && (
        <p role="alert" className="mt-2 text-xs" style={{ color: "var(--danger)" }}>
          Last error: {s.last_error}
        </p>
      )}
      <p className="mt-3 text-xs" style={{ color: "var(--text-muted)" }}>
        <kbd>Tab</kbd> accepts a word · <kbd>Shift</kbd>+<kbd>Tab</kbd> accepts all · <kbd>Esc</kbd> dismisses. Never in password fields.
      </p>
    </section>
  );
}

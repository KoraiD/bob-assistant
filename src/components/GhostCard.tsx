import { useCallback, useEffect, useState } from "react";
import { isTauri } from "../lib/appInfo";
import {
  api,
  ghostSummary,
  quietAppRows,
  type AppRef,
  type GhostMemory,
  type GhostStatus,
  type Snapshot,
} from "../lib/brains";
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
      {status && <QuietApps status={status} onChanged={refresh} />}
      {status && <MemoryList count={status.memory_count} onChanged={refresh} />}
      <p className="mt-3 text-xs" style={{ color: "var(--text-muted)" }}>
        <kbd>Tab</kbd> accepts a word · <kbd>Shift</kbd>+<kbd>Tab</kbd> accepts all · <kbd>Esc</kbd> dismisses. Never in password fields.
      </p>
    </section>
  );
}

const muted = { color: "var(--text-muted)" } as const;

/** Per-app switches: quiet apps first, then apps seen recently. */
function QuietApps({ status, onChanged }: { status: GhostStatus; onChanged: () => Promise<void> }) {
  const rows = quietAppRows(status);
  const [error, setError] = useState<string | null>(null);
  const flip = async (app: AppRef, quiet: boolean) => {
    try {
      setError(null);
      await api.ghostSetQuiet(app, quiet);
      await onChanged();
    } catch (e) {
      setError(String(e));
    }
  };
  return (
    <details className="mt-4" data-testid="quiet-apps">
      <summary className="cursor-pointer text-sm">
        Quiet apps <span style={muted}>({status.quiet_apps.length})</span>
      </summary>
      <p className="mt-2 text-xs" style={muted}>
        Ghost Text never suggests in these apps. Apps you used recently appear here so you can silence them.
      </p>
      <ul className="mt-2 space-y-1">
        {rows.map((app) => (
          <li key={app.bundle_id} className="flex items-center justify-between gap-3 text-sm">
            <span title={app.bundle_id}>{app.name || app.bundle_id}</span>
            <button
              type="button"
              role="switch"
              aria-checked={app.quiet}
              aria-label={`Quiet in ${app.name || app.bundle_id}`}
              onClick={() => void flip(app, !app.quiet)}
              className="rounded-full px-3 py-0.5 text-xs"
              style={app.quiet ? { background: "var(--surface-raised)", border: "1px solid var(--border-strong)" } : muted}
            >
              {app.quiet ? "Quiet" : "Haunting"}
            </button>
          </li>
        ))}
      </ul>
      {error && (
        <p role="alert" className="mt-2 text-xs" style={{ color: "var(--danger)" }}>
          {error}
        </p>
      )}
    </details>
  );
}

/** Accepted-phrase memory: count, view on demand, clear. Contents are only fetched when opened. */
function MemoryList({ count, onChanged }: { count: number; onChanged: () => Promise<void> }) {
  const [memory, setMemory] = useState<GhostMemory | null>(null);
  const [error, setError] = useState<string | null>(null);
  const load = async () => {
    try {
      setError(null);
      setMemory(await api.ghostMemory());
    } catch (e) {
      setError(String(e));
    }
  };
  const forget = async (text: string) => {
    try {
      setError(null);
      await api.ghostMemoryForget(text);
      setMemory((m) => (m ? { phrases: m.phrases.filter((p) => p.text !== text) } : m));
      await onChanged();
    } catch (e) {
      setError(String(e));
    }
  };
  const clear = async () => {
    try {
      setError(null);
      await api.ghostMemoryClear();
      setMemory({ phrases: [] });
      await onChanged();
    } catch (e) {
      setError(String(e));
    }
  };
  return (
    <details
      className="mt-3"
      data-testid="ghost-memory"
      onToggle={(e) => {
        if ((e.currentTarget as HTMLDetailsElement).open) void load();
      }}
    >
      <summary className="cursor-pointer text-sm">
        Remembered phrases <span style={muted}>({count})</span>
      </summary>
      <p className="mt-2 text-xs" style={muted}>
        Phrases you accepted, kept only on this Mac. Anything that looks like a secret is never stored.
      </p>
      {memory && memory.phrases.length === 0 && (
        <p className="mt-2 text-xs" style={muted}>
          Nothing remembered yet.
        </p>
      )}
      {memory && memory.phrases.length > 0 && (
        <ul className="mt-2 max-h-40 space-y-0.5 overflow-auto text-xs" aria-label="Remembered phrases">
          {memory.phrases.map((p) => (
            <li key={p.text} className="flex items-center justify-between gap-2">
              <span>
                {p.text} {p.uses > 1 && <span style={muted}>×{p.uses}</span>}
              </span>
              <button
                type="button"
                aria-label={`Forget "${p.text}"`}
                title="Forget this phrase"
                onClick={() => void forget(p.text)}
                className="rounded-full px-2"
                style={muted}
              >
                ✕
              </button>
            </li>
          ))}
        </ul>
      )}
      {count > 0 && (
        <button
          type="button"
          onClick={() => void clear()}
          className="mt-2 rounded-full px-3 py-0.5 text-xs"
          style={{ border: "1px solid var(--border-strong)", color: "var(--danger)" }}
        >
          Forget all
        </button>
      )}
      {error && (
        <p role="alert" className="mt-2 text-xs" style={{ color: "var(--danger)" }}>
          {error}
        </p>
      )}
    </details>
  );
}

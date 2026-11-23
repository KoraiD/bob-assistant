import { useCallback, useEffect, useState } from "react";
import { isTauri } from "../lib/appInfo";
import { HOTKEYS, hotkeyPreset, MAX_CUSTOM, selectionApi, validCustom, type CustomAction, type SelectionView } from "../lib/selection";

const muted = { color: "var(--text-muted)" } as const;
const field = { background: "var(--surface-raised)", border: "1px solid var(--border)" } as const;

/** Overview card for Selection Actions: hotkey, the action list, custom actions and usage. */
export function SelectionCard() {
  const [view, setView] = useState<SelectionView | null>(null);
  const [drafts, setDrafts] = useState<CustomAction[] | null>(null);
  const [status, setStatus] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!isTauri()) return;
    try {
      setView(await selectionApi.get());
    } catch {
      /* the next tick retries */
    }
  }, []);

  useEffect(() => {
    void refresh();
    const t = setInterval(() => void refresh(), 3000);
    return () => clearInterval(t);
  }, [refresh]);

  if (!view) {
    return (
      <section aria-label="Selection Actions" className="soft-card panel-in mt-6 p-5">
        <h2 className="text-lg font-medium">Selection Actions</h2>
        <p className="text-sm" style={muted}>
          {isTauri() ? "Checking..." : "Open the BOB app to use Selection Actions."}
        </p>
      </section>
    );
  }

  const preset = hotkeyPreset(view.settings.hotkey);
  const custom = drafts ?? view.settings.custom;
  const save = async (patch: Partial<SelectionView["settings"]>) => {
    try {
      await selectionApi.setSettings({ ...view.settings, ...patch });
      setStatus("Saved");
      setDrafts(null);
      await refresh();
    } catch (e) {
      setStatus(String(e));
    }
  };
  const edit = (i: number, patch: Partial<CustomAction>) => setDrafts(custom.map((c, j) => (j === i ? { ...c, ...patch } : c)));
  const s = view.stats;

  return (
    <section aria-label="Selection Actions" className="soft-card panel-in mt-6 p-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-lg font-medium">Selection Actions</h2>
          <p className="text-sm" style={muted} data-testid="selection-summary">
            Select text in any app and press <kbd>{preset?.label ?? "your hotkey"}</kbd>. Pick an action with ↑↓ and ⏎; the result replaces the selection (⌘Z undoes it).
          </p>
        </div>
        <label className="flex items-center gap-2 text-sm">
          Hotkey
          <select
            aria-label="Selection Actions hotkey"
            value={preset?.id ?? "custom"}
            onChange={(e) => {
              const p = HOTKEYS.find((h) => h.id === e.currentTarget.value);
              if (p) void save({ hotkey: p.hotkey });
            }}
            className="rounded-lg px-2 py-1"
            style={field}
          >
            {HOTKEYS.map((h) => (
              <option key={h.id} value={h.id}>
                {h.label}
              </option>
            ))}
            {!preset && <option value="custom">Custom</option>}
          </select>
        </label>
      </div>

      <p className="mt-3 text-xs" style={muted} data-testid="selection-actions">
        Actions: {view.actions.join(" · ")}
      </p>
      {s.menus > 0 && (
        <p className="mt-1 text-xs" style={muted} data-testid="selection-stats">
          {s.runs} run · {s.replaced} replaced · {s.failed} failed{s.last_ms !== null ? ` · last took ${(s.last_ms / 1000).toFixed(1)} s` : ""}
        </p>
      )}
      {s.last_error && (
        <p role="alert" className="mt-1 text-xs" style={{ color: "var(--danger)" }}>
          Last problem: {s.last_error}
        </p>
      )}

      <details className="mt-4" data-testid="custom-actions">
        <summary className="cursor-pointer text-sm">
          Your own actions <span style={muted}>({view.settings.custom.length})</span>
        </summary>
        <p className="mt-2 text-xs" style={muted}>
          Give each action a name and tell BOB what to do, for example “Reply politely and decline”. They appear after the built-in actions.
        </p>
        <ul className="mt-2 space-y-2">
          {custom.map((c, i) => (
            <li key={i} className="flex flex-wrap gap-2">
              <input aria-label={`Action ${i + 1} name`} value={c.label} maxLength={40} placeholder="Name" onChange={(e) => edit(i, { label: e.currentTarget.value })} className="w-40 rounded-lg px-2 py-1 text-sm" style={field} />
              <input aria-label={`Action ${i + 1} instruction`} value={c.instruction} maxLength={1000} placeholder="What BOB should do with the selected text" onChange={(e) => edit(i, { instruction: e.currentTarget.value })} className="min-w-64 flex-1 rounded-lg px-2 py-1 text-sm" style={field} />
              <button type="button" aria-label={`Remove action ${i + 1}`} onClick={() => setDrafts(custom.filter((_, j) => j !== i))} className="rounded-full px-3 text-xs" style={field}>
                Remove
              </button>
            </li>
          ))}
        </ul>
        <div className="mt-2 flex items-center gap-2 text-xs">
          <button type="button" disabled={custom.length >= MAX_CUSTOM} onClick={() => setDrafts([...custom, { label: "", instruction: "" }])} className="rounded-full px-3 py-1 disabled:opacity-50" style={field}>
            Add action
          </button>
          <button
            type="button"
            disabled={drafts === null}
            onClick={() => void save({ custom: validCustom(custom) })}
            className="rounded-full px-3 py-1 disabled:opacity-50"
            style={drafts ? { background: "var(--accent)", color: "var(--accent-fg)" } : field}
          >
            Save actions
          </button>
          {status && <span role="status">{status}</span>}
        </div>
      </details>
    </section>
  );
}

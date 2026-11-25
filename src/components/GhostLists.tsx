import { useMemo, useState } from "react";
import { api, type AppRef, type GhostMemory, type GhostStatus } from "../lib/brains";

const muted = { color: "var(--text-muted)" } as const;
const field = { background: "var(--surface-raised)", border: "1px solid var(--border)" } as const;
const danger = { color: "var(--danger)" } as const;

/** Same rule as `GhostSettings::set_quiet` in Rust. */
export function validBundleId(id: string): boolean {
  const t = id.trim();
  return t.length > 0 && t.length <= 255 && /^[A-Za-z0-9._-]+$/.test(t) && t.includes(".");
}

const matches = (q: string, ...texts: string[]) => {
  const needle = q.trim().toLowerCase();
  return !needle || texts.some((t) => t.toLowerCase().includes(needle));
};

/** Apps you could silence: running and recently used apps that are not quiet yet, by name. */
export function suggestions(status: Pick<GhostStatus, "quiet_apps" | "recent_apps">, running: AppRef[]): AppRef[] {
  const taken = new Set(status.quiet_apps.map((a) => a.bundle_id));
  const out: AppRef[] = [];
  for (const a of [...status.recent_apps, ...running]) {
    if (!a.bundle_id || taken.has(a.bundle_id)) continue;
    taken.add(a.bundle_id);
    out.push(a);
  }
  return out.sort((a, b) => (a.name || a.bundle_id).localeCompare(b.name || b.bundle_id));
}

function ErrorLine({ error }: { error: string | null }) {
  return error ? (
    <p role="alert" className="mt-2 text-xs" style={danger}>
      {error}
    </p>
  ) : null;
}

/** Quiet apps: the list with remove buttons, one-click add from running or recent apps, or a bundle id. */
export function QuietAppsEditor({ status, onChanged }: { status: GhostStatus; onChanged: () => Promise<void> }) {
  const [running, setRunning] = useState<AppRef[]>([]);
  const [query, setQuery] = useState("");
  const [manual, setManual] = useState("");
  const [error, setError] = useState<string | null>(null);

  const set = async (app: AppRef, quiet: boolean) => {
    try {
      setError(null);
      await api.ghostSetQuiet(app, quiet);
      await onChanged();
    } catch (e) {
      setError(String(e));
    }
  };
  const addManual = async () => {
    const id = manual.trim();
    if (!validBundleId(id)) {
      setError("A bundle id looks like com.company.App (letters, digits, dots and dashes).");
      return;
    }
    setManual("");
    await set({ bundle_id: id, name: "" }, true);
  };
  const quiet = status.quiet_apps.filter((a) => matches(query, a.name, a.bundle_id));
  const add = useMemo(() => suggestions(status, running), [status, running]).filter((a) => matches(query, a.name, a.bundle_id));

  return (
    <details
      className="mt-4"
      data-testid="quiet-apps"
      onToggle={(e) => {
        if ((e.currentTarget as HTMLDetailsElement).open) {
          api.runningApps().then(setRunning, () => setRunning([]));
        }
      }}
    >
      <summary className="cursor-pointer text-sm">
        Quiet apps <span style={muted}>({status.quiet_apps.length})</span>
      </summary>
      <p className="mt-2 text-xs" style={muted}>
        Ghost Text never suggests in these apps. Add one from the apps you are using, or type its bundle id.
      </p>
      <input
        type="search"
        aria-label="Search apps"
        placeholder="Search apps"
        value={query}
        onChange={(e) => setQuery(e.currentTarget.value)}
        className="mt-2 w-full rounded-lg px-2 py-1 text-sm"
        style={field}
      />
      <ul className="mt-2 max-h-48 space-y-1 overflow-auto" aria-label="Quiet apps">
        {quiet.map((app) => (
          <li key={app.bundle_id} className="flex items-center justify-between gap-3 text-sm">
            <span className="min-w-0 truncate" title={app.bundle_id}>
              {app.name || app.bundle_id}
              {app.name && (
                <span className="ml-2 text-xs" style={muted}>
                  {app.bundle_id}
                </span>
              )}
            </span>
            <button
              type="button"
              aria-label={`Let Ghost Text haunt ${app.name || app.bundle_id} again`}
              title="Remove from quiet apps"
              onClick={() => void set(app, false)}
              className="shrink-0 rounded-full px-2.5 py-0.5 text-xs"
              style={field}
            >
              Remove
            </button>
          </li>
        ))}
        {quiet.length === 0 && (
          <li className="text-xs" style={muted}>
            {query ? "No quiet app matches." : "No quiet apps."}
          </li>
        )}
      </ul>
      {add.length > 0 && (
        <>
          <p className="mt-3 text-xs" style={muted}>
            Silence another app:
          </p>
          <div className="mt-1 flex max-h-28 flex-wrap gap-1.5 overflow-auto" role="group" aria-label="Apps you can silence">
            {add.map((app) => (
              <button
                key={app.bundle_id}
                type="button"
                aria-label={`Quiet in ${app.name || app.bundle_id}`}
                title={app.bundle_id}
                onClick={() => void set(app, true)}
                className="rounded-full px-2.5 py-0.5 text-xs"
                style={field}
              >
                + {app.name || app.bundle_id}
              </button>
            ))}
          </div>
        </>
      )}
      <form
        className="mt-3 flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void addManual();
        }}
      >
        <input
          aria-label="Bundle id"
          placeholder="Bundle id, e.g. com.apple.Terminal"
          value={manual}
          maxLength={255}
          onChange={(e) => setManual(e.currentTarget.value)}
          className="min-w-0 flex-1 rounded-lg px-2 py-1 text-sm"
          style={field}
        />
        <button type="submit" disabled={!manual.trim()} className="rounded-full px-3 text-xs disabled:opacity-50" style={field}>
          Add
        </button>
      </form>
      <ErrorLine error={error} />
    </details>
  );
}

/** Remembered phrases: add, edit inline, delete, search, forget all (with a confirmation). Loaded on open. */
export function PhraseEditor({ count, onChanged }: { count: number; onChanged: () => Promise<void> }) {
  const [memory, setMemory] = useState<GhostMemory | null>(null);
  const [draft, setDraft] = useState("");
  const [query, setQuery] = useState("");
  const [editing, setEditing] = useState<{ old: string; text: string } | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async (f: () => Promise<void>) => {
    try {
      setError(null);
      await f();
    } catch (e) {
      setError(String(e));
    }
  };
  const load = () => run(async () => setMemory(await api.ghostMemory()));
  const reload = async () => {
    setMemory(await api.ghostMemory());
    await onChanged();
  };
  const KEEP_RULE = "BOB keeps phrases with at least two letters or digits, and never anything that looks like a password or key.";
  const add = () =>
    run(async () => {
      const t = draft.trim();
      if (!t) return;
      if (!(await api.ghostMemoryAdd(t))) return setError(KEEP_RULE);
      setDraft("");
      await reload();
    });
  const saveEdit = () =>
    run(async () => {
      if (!editing) return;
      if (editing.text.trim() === editing.old) return setEditing(null);
      if (!(await api.ghostMemoryEdit(editing.old, editing.text))) return setError(KEEP_RULE);
      setEditing(null);
      await reload();
    });
  const forget = (text: string) =>
    run(async () => {
      await api.ghostMemoryForget(text);
      setMemory((m) => (m ? { phrases: m.phrases.filter((p) => p.text !== text) } : m));
      await onChanged();
    });
  const clear = () =>
    run(async () => {
      await api.ghostMemoryClear();
      setConfirmClear(false);
      setMemory({ phrases: [] });
      await onChanged();
    });

  const phrases = (memory?.phrases ?? []).filter((p) => matches(query, p.text));
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
        Phrases you accepted or added, kept only on this Mac. Ghost Text uses them when memory is on (Personalize).
      </p>
      <form
        className="mt-2 flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void add();
        }}
      >
        <input
          aria-label="New phrase"
          placeholder="Add a phrase, e.g. Best regards, Zsófi"
          value={draft}
          maxLength={64}
          onChange={(e) => setDraft(e.currentTarget.value)}
          className="min-w-0 flex-1 rounded-lg px-2 py-1 text-sm"
          style={field}
        />
        <button type="submit" disabled={!draft.trim()} className="rounded-full px-3 text-xs disabled:opacity-50" style={field}>
          Add
        </button>
      </form>
      {memory && memory.phrases.length > 5 && (
        <input
          type="search"
          aria-label="Search phrases"
          placeholder="Search phrases"
          value={query}
          onChange={(e) => setQuery(e.currentTarget.value)}
          className="mt-2 w-full rounded-lg px-2 py-1 text-sm"
          style={field}
        />
      )}
      {memory && memory.phrases.length === 0 && (
        <p className="mt-2 text-xs" style={muted}>
          Nothing remembered yet.
        </p>
      )}
      {memory && memory.phrases.length > 0 && (
        <ul className="mt-2 max-h-56 space-y-1 overflow-auto text-sm" aria-label="Remembered phrases">
          {phrases.map((p) =>
            editing?.old === p.text ? (
              <li key={p.text}>
                <form
                  className="flex gap-2"
                  onSubmit={(e) => {
                    e.preventDefault();
                    void saveEdit();
                  }}
                >
                  <input
                    aria-label={`Edit "${p.text}"`}
                    value={editing.text}
                    maxLength={64}
                    autoFocus
                    onChange={(e) => setEditing({ old: p.text, text: e.currentTarget.value })}
                    onKeyDown={(e) => {
                      if (e.key === "Escape") {
                        e.preventDefault();
                        setEditing(null);
                      }
                    }}
                    className="min-w-0 flex-1 rounded-lg px-2 py-0.5 text-sm"
                    style={field}
                  />
                  <button type="submit" className="rounded-full px-2.5 text-xs" style={{ background: "var(--accent)", color: "var(--accent-fg)" }}>
                    Save
                  </button>
                  <button type="button" onClick={() => setEditing(null)} className="rounded-full px-2.5 text-xs" style={field}>
                    Cancel
                  </button>
                </form>
              </li>
            ) : (
              <li key={p.text} className="flex items-center justify-between gap-2">
                <span className="min-w-0 truncate">
                  {p.text} {p.uses > 1 && <span style={muted}>×{p.uses}</span>}
                </span>
                <span className="flex shrink-0 gap-1">
                  <button
                    type="button"
                    aria-label={`Edit "${p.text}"`}
                    title="Edit"
                    onClick={() => setEditing({ old: p.text, text: p.text })}
                    className="rounded-full px-2 text-xs"
                    style={muted}
                  >
                    Edit
                  </button>
                  <button type="button" aria-label={`Forget "${p.text}"`} title="Forget this phrase" onClick={() => void forget(p.text)} className="rounded-full px-2" style={muted}>
                    ✕
                  </button>
                </span>
              </li>
            ),
          )}
          {phrases.length === 0 && (
            <li className="text-xs" style={muted}>
              No phrase matches.
            </li>
          )}
        </ul>
      )}
      {count > 0 &&
        (confirmClear ? (
          <div className="mt-2 flex items-center gap-2 text-xs" role="group" aria-label="Confirm forgetting">
            <span>
              Forget all {count} phrases? This cannot be undone.
            </span>
            <button type="button" onClick={() => void clear()} className="rounded-full px-3 py-0.5" style={{ border: "1px solid var(--border-strong)", ...danger }}>
              Forget all
            </button>
            <button type="button" onClick={() => setConfirmClear(false)} className="rounded-full px-3 py-0.5" style={field}>
              Keep them
            </button>
          </div>
        ) : (
          <button
            type="button"
            onClick={() => setConfirmClear(true)}
            className="mt-2 rounded-full px-3 py-0.5 text-xs"
            style={{ border: "1px solid var(--border-strong)", ...danger }}
          >
            Forget all…
          </button>
        ))}
      <ErrorLine error={error} />
    </details>
  );
}

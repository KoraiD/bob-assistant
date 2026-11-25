import { useCallback, useEffect, useRef, useState } from "react";
import { isTauri } from "../lib/appInfo";
import { useTauriEvent } from "../lib/useTauriEvent";
import {
  PROVIDERS,
  ROLES,
  api,
  applyEdit,
  brainsForRole,
  draftToConfig,
  editFrom,
  formatBytes,
  percent,
  sidecarLabel,
  validateDraft,
  validateEdit,
  type BrainEdit,
  type BrainView,
  type Draft,
  type ModelInfo,
  type ProgressEvent,
  type Snapshot,
  type TestReport,
  type TokenEvent,
} from "../lib/brains";

const muted = { color: "var(--text-muted)" };

function Chip({ children, tone = "accent" }: { children: React.ReactNode; tone?: "accent" | "pumpkin" | "muted" }) {
  const styles = {
    accent: { background: "var(--accent-soft)", color: "var(--accent)" },
    pumpkin: { background: "var(--accent-2-soft)", color: "var(--accent-2)" },
    muted: { background: "var(--border)", color: "var(--text-muted)" },
  }[tone];
  return (
    <span className="rounded-full px-2 py-0.5 text-xs" style={styles}>
      {children}
    </span>
  );
}

function Button(props: React.ButtonHTMLAttributes<HTMLButtonElement> & { primary?: boolean }) {
  const { primary, style, ...rest } = props;
  return (
    <button
      type="button"
      {...rest}
      className={`mac-button ${primary ? "primary" : "secondary"} ${rest.className ?? ""}`}
      style={style}
    />
  );
}

function ModelCatalog({ models, snapshot, refresh }: { models: ModelInfo[]; snapshot: Snapshot; refresh: () => void }) {
  const [progress, setProgress] = useState<Record<string, ProgressEvent>>({});
  const [error, setError] = useState<string | null>(null);
  const [starting, setStarting] = useState<string | null>(null);

  useTauriEvent<ProgressEvent>("model-progress", (p) => {
    setProgress((prev) => ({ ...prev, [p.id]: p }));
    if (p.done) {
      if (p.error) setError(p.error);
      refresh();
    }
  });

  const start = async (id: string) => {
    setError(null);
    setStarting(id);
    try {
      await api.startBuiltin(id);
    } catch (e) {
      setError(String(e));
    } finally {
      setStarting(null);
      refresh();
    }
  };

  return (
    <section className="mac-group-body panel-in mb-4 py-4" aria-label="Built-in models">
      <div className="mb-3 flex items-center justify-between">
        <div>
          <h3 className="font-medium">BOB's own brain</h3>
          <p className="text-xs" style={muted}>
            Runs on your Mac with llama.cpp. Nothing leaves your computer.
          </p>
        </div>
        <Chip tone={snapshot.builtin.state === "ready" ? "accent" : "pumpkin"}>{sidecarLabel(snapshot.builtin)}</Chip>
      </div>
      {snapshot.builtin.state === "failed" && (
        <p role="alert" className="mb-2 text-sm" style={{ color: "var(--danger)" }}>
          {snapshot.builtin.message}
        </p>
      )}
      <ul className="space-y-2">
        {models.map((m) => {
          const p = progress[m.id];
          const downloading = p && !p.done;
          return (
            <li key={m.id} className="flex items-center justify-between gap-3 rounded-xl p-3" style={{ background: "var(--surface-raised)" }} data-testid={`model-${m.id}`}>
              <div className="min-w-0">
                <div className="text-sm font-medium">
                  {m.name} {m.active && <Chip>in use</Chip>}
                </div>
                <div className="text-xs" style={muted}>
                  {m.role} · {formatBytes(m.size)} · {m.license}
                </div>
                {downloading && (
                  <div className="mt-2 h-1.5 w-56 overflow-hidden rounded-full" style={{ background: "var(--border)" }}>
                    <div
                      role="progressbar"
                      aria-valuenow={percent(p.downloaded, p.total)}
                      aria-valuemin={0}
                      aria-valuemax={100}
                      className="h-full"
                      style={{ width: `${percent(p.downloaded, p.total)}%`, background: "var(--accent)" }}
                    />
                  </div>
                )}
              </div>
              <div className="flex shrink-0 gap-2">
                {!m.installed && !downloading && <Button onClick={() => api.download(m.id).catch((e) => setError(String(e)))}>Download</Button>}
                {downloading && <Button onClick={() => api.cancel(m.id)}>Cancel {percent(p.downloaded, p.total)}%</Button>}
                {m.installed && !m.active && (
                  <Button primary disabled={starting !== null} onClick={() => start(m.id)}>
                    {starting === m.id ? "Waking up..." : "Use"}
                  </Button>
                )}
              </div>
            </li>
          );
        })}
      </ul>
      {error && (
        <p role="alert" className="mt-2 text-sm" style={{ color: "var(--danger)" }}>
          {error}
        </p>
      )}
    </section>
  );
}

function EditBrain({ brain, onDone }: { brain: BrainView; onDone: (saved: boolean) => void }) {
  const [e, setE] = useState<BrainEdit>(() => editFrom(brain));
  const [errors, setErrors] = useState<string[]>([]);
  const set = (patch: Partial<BrainEdit>) => setE((cur) => ({ ...cur, ...patch }));
  const save = async () => {
    const v = validateEdit(brain, e);
    setErrors(v);
    if (v.length) return;
    try {
      const { config, apiKey } = applyEdit(brain, e);
      await api.upsert(config, apiKey);
      onDone(true);
    } catch (err) {
      setErrors([String(err)]);
    }
  };
  const field = "w-full rounded-lg px-2 py-1.5 text-sm";
  const fieldStyle = { background: "var(--bg)", border: "1px solid var(--border-strong)", color: "var(--text)" };
  return (
    <form
      aria-label={`Edit ${brain.name}`}
      className="mt-3 grid gap-2 md:grid-cols-2"
      onSubmit={(ev) => {
        ev.preventDefault();
        void save();
      }}
      onKeyDown={(ev) => ev.key === "Escape" && onDone(false)}
    >
      <label className="text-xs" style={muted}>
        Name
        <input className={field} style={fieldStyle} value={e.name} onChange={(ev) => set({ name: ev.target.value })} autoFocus />
      </label>
      <label className="text-xs" style={muted}>
        Endpoint
        <input className={field} style={fieldStyle} value={e.endpoint} onChange={(ev) => set({ endpoint: ev.target.value })} />
      </label>
      <label className="text-xs" style={muted}>
        Model (optional)
        <input className={field} style={fieldStyle} value={e.model} onChange={(ev) => set({ model: ev.target.value })} />
      </label>
      <label className="text-xs" style={muted}>
        New API key {brain.has_api_key ? "(leave empty to keep the stored one)" : "(optional)"}
        <input className={field} style={fieldStyle} type="password" autoComplete="off" value={e.apiKey} onChange={(ev) => set({ apiKey: ev.target.value })} />
      </label>
      {brain.has_api_key && (
        <label className="flex items-center gap-2 text-xs md:col-span-2" style={muted}>
          <input type="checkbox" checked={e.removeKey} onChange={(ev) => set({ removeKey: ev.target.checked })} />
          Remove the stored key from the Keychain
        </label>
      )}
      {errors.length > 0 && (
        <ul role="alert" className="text-sm md:col-span-2" style={{ color: "var(--danger)" }}>
          {errors.map((x) => (
            <li key={x}>{x}</li>
          ))}
        </ul>
      )}
      <div className="flex gap-2 md:col-span-2">
        <Button primary type="submit">
          Save changes
        </Button>
        <Button onClick={() => onDone(false)}>Cancel</Button>
      </div>
    </form>
  );
}

function BrainCard({ brain, onChanged }: { brain: BrainView; onChanged: () => void }) {
  const [report, setReport] = useState<TestReport | null>(null);
  const [testing, setTesting] = useState(false);
  const [editing, setEditing] = useState(false);
  const [confirmRemove, setConfirmRemove] = useState(false);

  const test = async () => {
    setTesting(true);
    try {
      setReport(await api.test(brain.id));
    } catch (e) {
      setReport({ reachable: false, latency_ms: null, models: [], capabilities: brain.capabilities, is_cloud: brain.is_cloud, error: String(e) });
    } finally {
      setTesting(false);
      onChanged();
    }
  };

  return (
    <li className="rounded-xl p-3" style={{ background: "var(--surface-raised)" }} data-testid={`brain-${brain.id}`}>
      <div className="flex items-start justify-between gap-3">
        <div>
          <div className="flex flex-wrap items-center gap-2 text-sm font-medium">
            {brain.name}
            {brain.kind === "builtin" ? <Chip>on this Mac</Chip> : brain.is_cloud ? <Chip tone="pumpkin">cloud: text leaves your Mac</Chip> : <Chip>local</Chip>}
            {brain.capabilities.ghost && <Chip tone="muted">ghost</Chip>}
            {brain.capabilities.chat && <Chip tone="muted">chat</Chip>}
          </div>
          {brain.kind !== "builtin" && (
            <div className="text-xs" style={muted}>
              {brain.endpoint}
              {brain.model ? ` · ${brain.model}` : ""}
              {brain.has_api_key ? " · key in Keychain" : ""}
            </div>
          )}
        </div>
        <div className="flex shrink-0 gap-2">
          <Button onClick={test} disabled={testing}>
            {testing ? "Testing..." : "Test"}
          </Button>
          {brain.kind !== "builtin" && !editing && (
            <Button aria-label={`Edit ${brain.name}`} onClick={() => setEditing(true)}>
              Edit
            </Button>
          )}
          {brain.kind !== "builtin" &&
            (confirmRemove ? (
              <>
                <Button
                  aria-label={`Confirm removing ${brain.name}`}
                  style={{ color: "var(--danger)" }}
                  onClick={async () => {
                    await api.remove(brain.id);
                    onChanged();
                  }}
                >
                  Remove for good
                </Button>
                <Button onClick={() => setConfirmRemove(false)}>Keep</Button>
              </>
            ) : (
              <Button aria-label={`Remove ${brain.name}`} onClick={() => setConfirmRemove(true)}>
                Remove
              </Button>
            ))}
        </div>
      </div>
      {editing && (
        <EditBrain
          brain={brain}
          onDone={(saved) => {
            setEditing(false);
            if (saved) onChanged();
          }}
        />
      )}
      <label className="mt-2 flex items-center gap-2 text-xs" style={muted}>
        <input
          type="checkbox"
          checked={brain.shares_context}
          aria-label={`Share personal context with ${brain.name}`}
          onChange={async (e) => {
            await api.setShareContext(brain.id, e.currentTarget.checked);
            onChanged();
          }}
        />
        Share my profile, tone and visible context with this brain
        {brain.is_cloud && !brain.shares_context ? " (off: cloud brain)" : ""}
      </label>
      {report && (
        <p className="mt-2 text-xs" data-testid={`report-${brain.id}`} style={{ color: report.error ? "var(--danger)" : "var(--text-muted)" }}>
          {report.error
            ? report.error
            : `Reachable in ${report.latency_ms} ms${report.models.length ? ` · ${report.models.length} model(s)` : ""} · ghost ${report.capabilities.ghost ? "yes" : "no"} · chat ${report.capabilities.chat ? "yes" : "no"}`}
        </p>
      )}
    </li>
  );
}

const EMPTY: Draft = { name: "", kind: "ollama", endpoint: PROVIDERS[0].endpoint, model: "", apiKey: "" };

function AddBrain({ taken, onAdded }: { taken: string[]; onAdded: () => void }) {
  const [draft, setDraft] = useState<Draft>(EMPTY);
  const [errors, setErrors] = useState<string[]>([]);
  const set = (patch: Partial<Draft>) => setDraft((d) => ({ ...d, ...patch }));

  const save = async () => {
    const v = validateDraft(draft);
    setErrors(v);
    if (v.length) return;
    try {
      await api.upsert(draftToConfig(draft, taken), draft.apiKey.trim() || null);
      setDraft(EMPTY);
      onAdded();
    } catch (e) {
      setErrors([String(e)]);
    }
  };

  const field = "w-full rounded-lg px-2 py-1.5 text-sm";
  const fieldStyle = { background: "var(--bg)", border: "1px solid var(--border-strong)", color: "var(--text)" };
  return (
    <form
      aria-label="Add a brain"
      className="mt-4 grid gap-2 md:grid-cols-2"
      onSubmit={(e) => {
        e.preventDefault();
        void save();
      }}
    >
      <label className="text-xs" style={muted}>
        Provider
        <select
          className={field}
          style={fieldStyle}
          value={draft.kind}
          onChange={(e) => {
            const p = PROVIDERS.find((x) => x.kind === e.target.value)!;
            set({ kind: p.kind, endpoint: p.endpoint });
          }}
        >
          {PROVIDERS.map((p) => (
            <option key={p.kind} value={p.kind}>
              {p.label}
            </option>
          ))}
        </select>
      </label>
      <label className="text-xs" style={muted}>
        Name
        <input className={field} style={fieldStyle} value={draft.name} onChange={(e) => set({ name: e.target.value })} placeholder="My Ollama" />
      </label>
      <label className="text-xs" style={muted}>
        Endpoint
        <input className={field} style={fieldStyle} value={draft.endpoint} onChange={(e) => set({ endpoint: e.target.value })} />
      </label>
      <label className="text-xs" style={muted}>
        Model (optional)
        <input className={field} style={fieldStyle} value={draft.model} onChange={(e) => set({ model: e.target.value })} placeholder="qwen2.5:1.5b" />
      </label>
      <label className="text-xs md:col-span-2" style={muted}>
        API key (stored in the macOS Keychain, never in files)
        <input className={field} style={fieldStyle} type="password" autoComplete="off" value={draft.apiKey} onChange={(e) => set({ apiKey: e.target.value })} />
      </label>
      {errors.length > 0 && (
        <ul role="alert" className="text-sm md:col-span-2" style={{ color: "var(--danger)" }}>
          {errors.map((e) => (
            <li key={e}>{e}</li>
          ))}
        </ul>
      )}
      <div className="md:col-span-2">
        <Button primary type="submit">
          Add brain
        </Button>
      </div>
    </form>
  );
}

function Roles({ snapshot, onChanged }: { snapshot: Snapshot; onChanged: () => void }) {
  const [error, setError] = useState<string | null>(null);
  return (
    <section className="mac-group-body panel-in mb-4 py-4" aria-label="Which brain does what">
      <h3 className="mb-3 font-medium">Which brain does what</h3>
      <div className="grid gap-3 md:grid-cols-3">
        {ROLES.map((r) => (
          <label key={r.id} className="text-xs" style={muted}>
            {r.label}
            <select
              aria-label={`${r.label} brain`}
              className="mt-1 w-full rounded-lg px-2 py-1.5 text-sm"
              style={{ background: "var(--bg)", border: "1px solid var(--border-strong)", color: "var(--text)" }}
              value={snapshot.roles[r.id] ?? ""}
              onChange={async (e) => {
                setError(null);
                try {
                  await api.setRole(r.id, e.target.value || null);
                } catch (err) {
                  setError(String(err));
                }
                onChanged();
              }}
            >
              <option value="">Off</option>
              {brainsForRole(snapshot.brains, r.id).map((b) => (
                <option key={b.id} value={b.id}>
                  {b.name}
                  {b.is_cloud ? " (cloud)" : ""}
                </option>
              ))}
            </select>
          </label>
        ))}
      </div>
      {error && (
        <p role="alert" className="mt-2 text-sm" style={{ color: "var(--danger)" }}>
          {error}
        </p>
      )}
    </section>
  );
}

function TryIt({ snapshot }: { snapshot: Snapshot }) {
  const [brainId, setBrainId] = useState<string>("builtin");
  const [mode, setMode] = useState<"ghost" | "chat">("ghost");
  const [prompt, setPrompt] = useState("Thanks for your message. I will get back to you");
  const [output, setOutput] = useState("");
  const [meta, setMeta] = useState<string | null>(null);
  const [polished, setPolished] = useState<string | null>(null);
  const current = useRef<number | null>(null);

  useTauriEvent<TokenEvent>("brain-token", (t) => {
    if (t.request_id !== current.current) return;
    if (t.text) setOutput((o) => o + t.text);
    if (t.done) {
      setMeta(t.error ? `Error: ${t.error}` : `First token in ${t.first_token_ms ?? "?"} ms`);
      setPolished(t.polished);
    }
  });

  const run = async () => {
    setOutput("");
    setPolished(null);
    setMeta("Thinking...");
    try {
      current.current = await api.tryIt(brainId, prompt, mode);
    } catch (e) {
      setMeta(`Error: ${e}`);
    }
  };

  return (
    <section className="mac-group-body panel-in mb-4 py-4" aria-label="Try a brain">
      <h3 className="mb-3 font-medium">Try it</h3>
      <div className="mb-2 flex flex-wrap gap-2">
        <select aria-label="Brain to try" className="rounded-lg px-2 py-1.5 text-sm" style={{ background: "var(--bg)", border: "1px solid var(--border-strong)", color: "var(--text)" }} value={brainId} onChange={(e) => setBrainId(e.target.value)}>
          {snapshot.brains.map((b) => (
            <option key={b.id} value={b.id}>
              {b.name}
            </option>
          ))}
        </select>
        <select aria-label="Mode" className="rounded-lg px-2 py-1.5 text-sm" style={{ background: "var(--bg)", border: "1px solid var(--border-strong)", color: "var(--text)" }} value={mode} onChange={(e) => setMode(e.target.value as "ghost" | "chat")}>
          <option value="ghost">Continue my text (Ghost)</option>
          <option value="chat">Answer an instruction (chat)</option>
        </select>
        <Button primary onClick={run}>
          Summon
        </Button>
      </div>
      <textarea aria-label="Prompt" className="h-20 w-full rounded-lg p-2 text-sm" style={{ background: "var(--bg)", border: "1px solid var(--border-strong)", color: "var(--text)" }} value={prompt} onChange={(e) => setPrompt(e.target.value)} />
      {mode === "ghost" && polished !== null && (
        <p className="mt-2 text-sm" data-testid="try-polished">
          <span className="mr-2 text-xs" style={muted}>
            Ghost Text shows:
          </span>
          <span>{prompt}</span>
          <span style={{ color: "var(--accent)" }}>{polished}</span>
        </p>
      )}
      <p className="mt-2 min-h-6 text-sm" data-testid="try-output">
        {mode === "ghost" && (
          <span className="mr-2 text-xs" style={muted}>
            Raw model output:
          </span>
        )}
        <span style={{ color: mode === "ghost" ? "var(--text-muted)" : "var(--accent)" }}>{output}</span>
      </p>
      {meta && (
        <p className="text-xs" style={muted} data-testid="try-meta">
          {meta}
        </p>
      )}
    </section>
  );
}

/** "Bring Your Own Brain": manage models and providers, route features to brains, try them out. */
export function BrainPanel() {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [s, m] = await Promise.all([api.snapshot(), api.models()]);
      setSnapshot(s);
      setModels(m);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    if (isTauri()) void refresh();
  }, [refresh]);
  useTauriEvent("brains-changed", () => void refresh());

  // While the built-in brain wakes up, poll so the badge updates.
  useEffect(() => {
    if (snapshot?.builtin.state !== "starting") return;
    const t = setInterval(() => void refresh(), 1500);
    return () => clearInterval(t);
  }, [snapshot?.builtin.state, refresh]);

  if (!isTauri()) {
    return (
      <section className="mac-group-body panel-in mb-4 py-4" aria-label="Brains">
        <h3 className="sr-only">Brains</h3>
        <p className="text-sm" style={muted} data-testid="brains-preview">
          Open the BOB app to manage brains. This browser preview cannot reach models.
        </p>
      </section>
    );
  }
  if (!snapshot) return <p style={muted}>{error ?? "Summoning brains..."}</p>;

  return (
    <div className="space-y-4">
      <ModelCatalog models={models} snapshot={snapshot} refresh={refresh} />
      <section className="mac-group-body panel-in mb-4 py-4" aria-label="Brains">
        <h3 className="mb-3 font-medium">Your brains</h3>
        <ul className="space-y-2">
          {snapshot.brains.map((b) => (
            <BrainCard key={b.id} brain={b} onChanged={refresh} />
          ))}
        </ul>
        <AddBrain taken={snapshot.brains.map((b) => b.id)} onAdded={refresh} />
      </section>
      <Roles snapshot={snapshot} onChanged={refresh} />
      <TryIt snapshot={snapshot} />
    </div>
  );
}

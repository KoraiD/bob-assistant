import { useCallback, useEffect, useState } from "react";
import { isTauri } from "../lib/appInfo";
import { api, formatBytes, percent, type ProgressEvent } from "../lib/brains";
import {
  addWord,
  CLEANUPS,
  DICTATION_HOTKEYS,
  dictationApi,
  dictationSummary,
  LANGUAGES,
  MAX_REPLACEMENTS,
  UNLOAD_CHOICES,
  type DictationSettings,
  type DictationView,
  type Replacement,
} from "../lib/dictation";
import { hotkeyPreset } from "../lib/selection";
import { useTauriEvent } from "../lib/useTauriEvent";

const muted = { color: "var(--text-muted)" } as const;
const field = { background: "var(--surface-raised)", border: "1px solid var(--border)" } as const;

function presetFor(h: DictationSettings["hotkey"]) {
  const key = (x: DictationSettings["hotkey"]) => `${x.key_code}:${[...x.modifiers].sort().join("+")}`;
  return DICTATION_HOTKEYS.find((p) => key(p.hotkey) === key(h));
}

/** Overview card for Voice Dictation: on/off, hotkey, speech model, language, cleanup and timings. */
export function DictationCard() {
  const [view, setView] = useState<DictationView | null>(null);
  const [progress, setProgress] = useState<Record<string, ProgressEvent>>({});
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!isTauri()) return;
    try {
      setView(await dictationApi.get());
    } catch {
      /* the next tick retries */
    }
  }, []);
  useEffect(() => {
    void refresh();
    const t = setInterval(() => void refresh(), 2000);
    return () => clearInterval(t);
  }, [refresh]);
  useTauriEvent<ProgressEvent>("model-progress", (p) => {
    setProgress((cur) => ({ ...cur, [p.id]: p }));
    if (p.done) {
      if (p.error) setError(p.error);
      void refresh();
    }
  });

  if (!view) {
    return (
      <section aria-label="Voice Dictation" className="soft-card panel-in mt-6 p-5">
        <h2 className="text-lg font-medium">Voice Dictation</h2>
        <p className="text-sm" style={muted}>
          {isTauri() ? "Checking..." : "Open the BOB app to use Voice Dictation."}
        </p>
      </section>
    );
  }

  const s = view.settings;
  const preset = presetFor(s.hotkey) ?? hotkeyPreset(s.hotkey);
  const save = async (patch: Partial<DictationSettings>) => {
    try {
      setError(null);
      await dictationApi.setSettings({ ...s, ...patch });
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };
  const st = view.stats;

  return (
    <section aria-label="Voice Dictation" className="soft-card panel-in mt-6 p-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-lg font-medium">Voice Dictation</h2>
          <p className="text-sm" style={muted} data-testid="dictation-summary">
            {dictationSummary(view, preset?.label ?? "your hotkey")}
          </p>
        </div>
        <button
          type="button"
          role="switch"
          aria-checked={s.enabled}
          aria-label="Voice Dictation"
          onClick={() => void save({ enabled: !s.enabled })}
          className="rounded-full px-4 py-1.5 text-sm font-medium"
          style={s.enabled ? { background: "var(--accent)", color: "var(--accent-fg)" } : { ...field, color: "var(--text-muted)" }}
        >
          {s.enabled ? "On" : "Off"}
        </button>
      </div>

      <div className="mt-4 flex flex-wrap gap-4 text-sm">
        <label className="flex items-center gap-2">
          Hotkey
          <select aria-label="Dictation hotkey" value={presetFor(s.hotkey)?.id ?? "custom"} onChange={(e) => {
            const p = DICTATION_HOTKEYS.find((h) => h.id === e.currentTarget.value);
            if (p) void save({ hotkey: p.hotkey });
          }} className="rounded-lg px-2 py-1" style={field}>
            {DICTATION_HOTKEYS.map((h) => (
              <option key={h.id} value={h.id}>{h.label}</option>
            ))}
            {!presetFor(s.hotkey) && <option value="custom">Custom</option>}
          </select>
        </label>
        <label className="flex items-center gap-2">
          Language
          <select aria-label="Dictation language" value={s.language} onChange={(e) => void save({ language: e.currentTarget.value })} className="rounded-lg px-2 py-1" style={field}>
            {LANGUAGES.map((l) => (
              <option key={l.code} value={l.code}>{l.label}</option>
            ))}
          </select>
        </label>
        <label className="flex items-center gap-2">
          After speaking
          <select aria-label="Dictation cleanup" value={s.cleanup} onChange={(e) => void save({ cleanup: e.currentTarget.value as DictationSettings["cleanup"] })} className="rounded-lg px-2 py-1" style={field}>
            {CLEANUPS.map((c) => (
              <option key={c.id} value={c.id}>{c.label}</option>
            ))}
          </select>
        </label>
      </div>
      {s.cleanup !== "none" && (
        <p className="mt-1 text-xs" style={muted}>
          Uses the Dictation brain (Brains tab). If it is not ready, BOB types what you said unchanged.
        </p>
      )}
      <div className="mt-3 flex flex-wrap items-center gap-4 text-sm">
        <label className="flex items-center gap-2">
          <input type="checkbox" checked={s.tap_to_toggle} onChange={(e) => void save({ tap_to_toggle: e.currentTarget.checked })} aria-label="Tap to start and stop" />
          Tap to start, tap again to stop
        </label>
        <label className="flex items-center gap-2">
          Free the model's memory
          <select aria-label="Unload the speech model" value={s.unload_after_min} onChange={(e) => void save({ unload_after_min: Number(e.currentTarget.value) })} className="rounded-lg px-2 py-1" style={field}>
            {UNLOAD_CHOICES.map((c) => (
              <option key={c.minutes} value={c.minutes}>{c.label}</option>
            ))}
            {!UNLOAD_CHOICES.some((c) => c.minutes === s.unload_after_min) && <option value={s.unload_after_min}>After {s.unload_after_min} idle minutes</option>}
          </select>
        </label>
      </div>
      <p className="mt-2 text-xs" style={muted} data-testid="voice-edit-hint">
        Voice edit: select text first, then hold {preset?.label ?? "the hotkey"} and say what to change, for example “make this friendlier” or “translate to German”. Uses the Selection Actions brain.
      </p>
      {st.last_available && <LastDictation onError={setError} />}
      <Vocabulary words={s.vocabulary} onSave={(vocabulary) => save({ vocabulary })} />
      <Replacements rules={s.replacements} onSave={(replacements) => save({ replacements })} />

      <ul className="mt-4 space-y-2" aria-label="Speech models">
        {view.models.map((m) => {
          const p = progress[m.id];
          const downloading = p && !p.done;
          const chosen = m.id === s.model;
          return (
            <li key={m.id} className="flex flex-wrap items-center justify-between gap-2 rounded-xl p-2 text-sm" style={{ background: "var(--surface-raised)" }} data-testid={`speech-${m.id}`}>
              <label className="flex items-center gap-2">
                <input type="radio" name="speech-model" checked={chosen} onChange={() => void save({ model: m.id })} aria-label={`Use ${m.name}`} />
                <span>
                  <span className="font-medium">{m.name}</span>{" "}
                  <span className="text-xs" style={muted}>
                    {formatBytes(m.size)} · {m.license} · {m.role}
                  </span>
                </span>
              </label>
              {m.installed ? (
                <span className="text-xs" style={muted}>{chosen ? (st.model_loaded ? "loaded" : "installed") : "installed"}</span>
              ) : downloading ? (
                <span className="flex items-center gap-2 text-xs">
                  {percent(p.downloaded, p.total)}%
                  <button type="button" onClick={() => void api.cancel(m.id)} className="rounded-full px-3" style={field}>
                    Cancel
                  </button>
                </span>
              ) : (
                <button type="button" onClick={() => void api.download(m.id).catch((e) => setError(String(e)))} className="rounded-full px-3 py-0.5 text-xs" style={field}>
                  Download
                </button>
              )}
            </li>
          );
        })}
      </ul>

      {st.dictations > 0 && (
        <p className="mt-3 text-xs" style={muted} data-testid="dictation-stats">
          {st.dictations} dictated · last: {((st.last_audio_ms ?? 0) / 1000).toFixed(1)} s of speech, transcribed in {st.last_transcribe_ms ?? "-"} ms, typed {st.last_release_to_insert_ms ?? "-"} ms after release
        </p>
      )}
      {(error || st.last_error) && (
        <p role="alert" className="mt-2 text-xs" style={{ color: "var(--danger)" }}>
          {error ?? st.last_error}
        </p>
      )}
      <p className="mt-3 text-xs" style={muted}>
        Speech is recognized on this Mac by whisper.cpp. Audio is never saved or sent anywhere. Never types into password fields.
      </p>
    </section>
  );
}

/** Copy the last dictation (kept in memory for two minutes) in case it landed in the wrong place. */
function LastDictation({ onError }: { onError: (e: string) => void }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    try {
      const text = await dictationApi.last();
      if (!text) return onError("The last dictation is gone (BOB keeps it for two minutes).");
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch (e) {
      onError(String(e));
    }
  };
  return (
    <p className="mt-2 flex items-center gap-2 text-xs" style={muted}>
      <button type="button" onClick={() => void copy()} className="rounded-full px-3 py-0.5" style={field}>
        {copied ? "Copied" : "Copy last dictation"}
      </button>
      Or choose Type Last Dictation Again in the menu bar. Forgotten after two minutes.
    </p>
  );
}

/** Names and terms whisper should expect. Saved on every change. */
function Vocabulary({ words, onSave }: { words: string[]; onSave: (w: string[]) => Promise<void> }) {
  const [draft, setDraft] = useState("");
  const add = async () => {
    const next = draft
      .split(",")
      .reduce((list, w) => addWord(list, w), words);
    setDraft("");
    if (next !== words) await onSave(next);
  };
  return (
    <details className="mt-4" data-testid="vocabulary">
      <summary className="cursor-pointer text-sm">
        Vocabulary <span style={muted}>({words.length})</span>
      </summary>
      <p className="mt-2 text-xs" style={muted}>
        Names and terms BOB should recognize, like “Kiroween” or a colleague's name. Separate several with commas.
      </p>
      {words.length > 0 && (
        <ul className="mt-2 flex flex-wrap gap-1.5" aria-label="Vocabulary">
          {words.map((w) => (
            <li key={w} className="flex items-center gap-1 rounded-full px-2.5 py-0.5 text-xs" style={field}>
              {w}
              <button type="button" aria-label={`Remove ${w}`} onClick={() => void onSave(words.filter((x) => x !== w))} style={muted}>
                ✕
              </button>
            </li>
          ))}
        </ul>
      )}
      <form
        className="mt-2 flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void add();
        }}
      >
        <input aria-label="New word" placeholder="Add a word or name" value={draft} maxLength={400} onChange={(e) => setDraft(e.currentTarget.value)} className="min-w-0 flex-1 rounded-lg px-2 py-1 text-sm" style={field} />
        <button type="submit" disabled={!draft.trim()} className="rounded-full px-3 text-xs disabled:opacity-50" style={field}>
          Add
        </button>
      </form>
    </details>
  );
}

/** "Say this, type that" rules, edited as rows and saved together. */
function Replacements({ rules, onSave }: { rules: Replacement[]; onSave: (r: Replacement[]) => Promise<void> }) {
  const [drafts, setDrafts] = useState<Replacement[] | null>(null);
  const rows = drafts ?? rules;
  const edit = (i: number, patch: Partial<Replacement>) => setDrafts(rows.map((r, j) => (j === i ? { ...r, ...patch } : r)));
  const save = async () => {
    await onSave(rows.filter((r) => r.from.trim()));
    setDrafts(null);
  };
  return (
    <details className="mt-3" data-testid="replacements">
      <summary className="cursor-pointer text-sm">
        Text replacements <span style={muted}>({rules.length})</span>
      </summary>
      <p className="mt-2 text-xs" style={muted}>
        When you say the words on the left, BOB types the text on the right, for example “my email” → your address.
      </p>
      <ul className="mt-2 space-y-2">
        {rows.map((r, i) => (
          <li key={i} className="flex flex-wrap items-center gap-2">
            <input aria-label={`Replacement ${i + 1}: when I say`} placeholder="When I say" value={r.from} maxLength={64} onChange={(e) => edit(i, { from: e.currentTarget.value })} className="w-40 rounded-lg px-2 py-1 text-sm" style={field} />
            <span aria-hidden style={muted}>→</span>
            <input aria-label={`Replacement ${i + 1}: type`} placeholder="Type this" value={r.to} maxLength={500} onChange={(e) => edit(i, { to: e.currentTarget.value })} className="min-w-48 flex-1 rounded-lg px-2 py-1 text-sm" style={field} />
            <button type="button" aria-label={`Remove replacement ${i + 1}`} onClick={() => setDrafts(rows.filter((_, j) => j !== i))} className="rounded-full px-3 text-xs" style={field}>
              Remove
            </button>
          </li>
        ))}
      </ul>
      <div className="mt-2 flex items-center gap-2 text-xs">
        <button type="button" disabled={rows.length >= MAX_REPLACEMENTS} onClick={() => setDrafts([...rows, { from: "", to: "" }])} className="rounded-full px-3 py-1 disabled:opacity-50" style={field}>
          Add replacement
        </button>
        <button
          type="button"
          disabled={drafts === null}
          onClick={() => void save()}
          className="rounded-full px-3 py-1 disabled:opacity-50"
          style={drafts ? { background: "var(--accent)", color: "var(--accent-fg)" } : field}
        >
          Save replacements
        </button>
      </div>
    </details>
  );
}

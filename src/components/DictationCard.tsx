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
import { Button, Group, Row, Toggle } from "./ui";

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
        <h3 className="sr-only">Voice Dictation</h3>
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

  const select = (label: string, value: string | number, onChange: (v: string) => void, options: { value: string | number; label: string }[]) => (
    <select aria-label={label} value={value} onChange={(e) => onChange(e.currentTarget.value)} className="mac-field">
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  );
  const hotkeys = DICTATION_HOTKEYS.map((h) => ({ value: h.id, label: h.label }));
  if (!presetFor(s.hotkey)) hotkeys.push({ value: "custom", label: "Custom" });
  const unload = UNLOAD_CHOICES.map((c) => ({ value: c.minutes, label: c.label }));
  if (!UNLOAD_CHOICES.some((c) => c.minutes === s.unload_after_min)) unload.push({ value: s.unload_after_min, label: `After ${s.unload_after_min} idle minutes` });

  return (
    <div className="max-w-3xl">
      <Group
        label="Voice Dictation"
        footer="Speech is recognized on this Mac by whisper.cpp. Audio is never saved or sent anywhere, and BOB never types into password fields."
      >
        <Row title="Voice Dictation" detail={<span data-testid="dictation-summary">{dictationSummary(view, preset?.label ?? "your hotkey")}</span>}>
          <Toggle label="Voice Dictation" on={s.enabled} onChange={(v) => void save({ enabled: v })} />
        </Row>
        <Row title="Hotkey">
          {select("Dictation hotkey", presetFor(s.hotkey)?.id ?? "custom", (v) => {
            const p = DICTATION_HOTKEYS.find((h) => h.id === v);
            if (p) void save({ hotkey: p.hotkey });
          }, hotkeys)}
        </Row>
        <Row title="Tap to start, tap again to stop" detail="Holding the hotkey always works too.">
          <Toggle label="Tap to start and stop" on={s.tap_to_toggle} onChange={(v) => void save({ tap_to_toggle: v })} />
        </Row>
        <Row title="Start and stop sounds">
          <Toggle label="Start and stop sounds" on={s.sounds} onChange={(v) => void save({ sounds: v })} />
        </Row>
        <Row title="Language">
          {select("Dictation language", s.language, (v) => void save({ language: v }), LANGUAGES.map((l) => ({ value: l.code, label: l.label })))}
        </Row>
        <Row title="After speaking" detail={s.cleanup !== "none" ? "Uses the Dictation brain (Brains). If it is not ready, BOB types what you said unchanged." : undefined}>
          {select("Dictation cleanup", s.cleanup, (v) => void save({ cleanup: v as DictationSettings["cleanup"] }), CLEANUPS.map((c) => ({ value: c.id, label: c.label })))}
        </Row>
        <Row title="Free the model's memory" detail="The first dictation after that takes about half a second longer.">
          {select("Unload the speech model", s.unload_after_min, (v) => void save({ unload_after_min: Number(v) }), unload)}
        </Row>
        {st.dictations > 0 && (
          <Row
            title="This session"
            detail={
              <span data-testid="dictation-stats">
                {st.dictations} dictated · last: {((st.last_audio_ms ?? 0) / 1000).toFixed(1)} s of speech, transcribed in {st.last_transcribe_ms ?? "-"} ms, typed {st.last_release_to_insert_ms ?? "-"} ms after release
              </span>
            }
          />
        )}
        {(error || st.last_error) && (
          <Row
            title="Last problem"
            detail={
              <span role="alert" style={{ color: "var(--danger)" }}>
                {error ?? st.last_error}
              </span>
            }
          />
        )}
        {st.last_available && <LastDictation onError={setError} />}
      </Group>

      <Group title="Speech model" label="Speech models">
        <ul aria-label="Speech models">
          {view.models.map((m) => {
            const p = progress[m.id];
            const downloading = p && !p.done;
            const chosen = m.id === s.model;
            return (
              <li key={m.id} className="mac-row" data-testid={`speech-${m.id}`}>
                <input type="radio" name="speech-model" checked={chosen} onChange={() => void save({ model: m.id })} aria-label={`Use ${m.name}`} />
                <div className="min-w-0 flex-1">
                  <div className="mac-row-title">{m.name}</div>
                  <div className="mac-row-detail">
                    {formatBytes(m.size)} · {m.license} · {m.role}
                  </div>
                </div>
                {m.installed ? (
                  <span className="shrink-0 text-xs" style={muted}>
                    {chosen && st.model_loaded ? "loaded" : "installed"}
                  </span>
                ) : downloading ? (
                  <span className="flex shrink-0 items-center gap-2 text-xs">
                    {percent(p.downloaded, p.total)}%
                    <Button onClick={() => void api.cancel(m.id)}>Cancel</Button>
                  </span>
                ) : (
                  <Button onClick={() => void api.download(m.id).catch((e) => setError(String(e)))}>Download</Button>
                )}
              </li>
            );
          })}
        </ul>
      </Group>

      <Group
        title="Words"
        label="Words"
        footer={
          <span data-testid="voice-edit-hint">
            Voice edit: select text, then hold {preset?.label ?? "the hotkey"} and say what to change, for example “make this friendlier” or “translate to German”. It uses the Selection Actions brain.
          </span>
        }
      >
        <div className="py-1">
          <Vocabulary words={s.vocabulary} onSave={(vocabulary) => save({ vocabulary })} />
          <Replacements rules={s.replacements} onSave={(replacements) => save({ replacements })} />
        </div>
      </Group>
    </div>
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
    <div className="mac-row">
      <div className="min-w-0 flex-1">
        <div className="mac-row-title">Last dictation</div>
        <div className="mac-row-detail">Kept for two minutes. Type Last Dictation Again is also in the menu bar.</div>
      </div>
      <Button onClick={() => void copy()}>{copied ? "Copied" : "Copy last dictation"}</Button>
    </div>
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
    <details data-testid="vocabulary">
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
    <details data-testid="replacements">
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

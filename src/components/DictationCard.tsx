import { useCallback, useEffect, useState } from "react";
import { isTauri } from "../lib/appInfo";
import { api, formatBytes, percent, type ProgressEvent } from "../lib/brains";
import { CLEANUPS, DICTATION_HOTKEYS, dictationApi, dictationSummary, LANGUAGES, type DictationSettings, type DictationView } from "../lib/dictation";
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
          Hold to talk
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

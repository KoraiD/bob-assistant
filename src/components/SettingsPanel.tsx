import { useCallback, useEffect, useState } from "react";
import { isTauri } from "../lib/appInfo";
import { generalApi, loginLabel, pauseLabel, SHELL_EVENT, type GeneralView, type PauseHow } from "../lib/general";
import { THEMES, type Theme } from "../lib/theme";
import { useTauriEvent } from "../lib/useTauriEvent";
import { GhostMark } from "./GhostMark";

const muted = { color: "var(--text-muted)" } as const;
const pill = { background: "var(--surface-raised)", border: "1px solid var(--border-strong)" } as const;

function Switch({ on, label, onChange, disabled }: { on: boolean; label: string; onChange: (v: boolean) => void; disabled?: boolean }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!on)}
      className="rounded-full px-4 py-1.5 text-sm font-medium disabled:opacity-50"
      style={on ? { background: "var(--accent)", color: "var(--accent-fg)" } : { ...pill, color: "var(--text-muted)" }}
    >
      {on ? "On" : "Off"}
    </button>
  );
}

function Row({ title, detail, children }: { title: string; detail: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-3 py-2">
      <div>
        <div className="text-sm font-medium">{title}</div>
        <div className="text-xs" style={muted}>
          {detail}
        </div>
      </div>
      {children}
    </div>
  );
}

/** Settings (Cmd+,): general Mac behavior, pause, appearance, the welcome tour, privacy and credits. */
export function SettingsPanel({ build, theme, onTheme, onTour }: { build: string; theme: Theme; onTheme: (t: Theme) => void; onTour: () => void }) {
  const [general, setGeneral] = useState<GeneralView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const refresh = useCallback(async () => {
    if (!isTauri()) return;
    try {
      setGeneral(await generalApi.get());
    } catch (e) {
      setError(String(e));
    }
  }, []);
  useEffect(() => void refresh(), [refresh]);
  useTauriEvent(SHELL_EVENT, () => void refresh());

  const run = async (f: () => Promise<unknown>) => {
    try {
      setError(null);
      await f();
    } catch (e) {
      setError(String(e));
    }
    await refresh();
  };
  const pauses: { how: PauseHow; label: string }[] = [
    { how: "hour", label: "For 1 hour" },
    { how: "tomorrow", label: "Until tomorrow" },
    { how: "indefinite", label: "Until I resume" },
  ];

  return (
    <div className="space-y-4">
      <section aria-label="General" className="soft-card panel-in p-5">
        <h2 className="mb-1 text-lg font-medium">General</h2>
        {!general ? (
          <p className="text-sm" style={muted}>
            {isTauri() ? "Loading..." : "Open the BOB app to change these."}
          </p>
        ) : (
          <div className="divide-y" style={{ borderColor: "var(--border)" }}>
            <Row title="Open at login" detail={loginLabel(general.login_item)}>
              <Switch
                label="Open at login"
                on={general.login_item === "enabled" || general.login_item === "requires_approval"}
                disabled={general.login_item === "unavailable"}
                onChange={(v) => void run(() => generalApi.setLogin(v))}
              />
            </Row>
            <Row title="Open the window at launch" detail="Off: BOB starts quietly in the menu bar.">
              <Switch label="Open the window at launch" on={general.open_window_at_launch} onChange={(v) => void run(() => generalApi.setOpenAtLaunch(v))} />
            </Row>
            <Row title="Show in the Dock" detail="Off: BOB lives only in the menu bar.">
              <Switch label="Show in the Dock" on={general.show_in_dock} onChange={(v) => void run(() => generalApi.setDock(v))} />
            </Row>
            <Row title="Pause BOB" detail={`${pauseLabel(general.pause)}. Turns off suggestions, Selection Actions and dictation, for screen sharing or a quiet hour.`}>
              {general.pause.paused ? (
                <button type="button" onClick={() => void run(generalApi.resume)} className="rounded-full px-4 py-1.5 text-sm font-medium" style={{ background: "var(--accent)", color: "var(--accent-fg)" }}>
                  Resume
                </button>
              ) : (
                <div className="flex flex-wrap gap-2">
                  {pauses.map((p) => (
                    <button key={p.how} type="button" onClick={() => void run(() => generalApi.pause(p.how))} className="rounded-full px-3 py-1 text-xs" style={pill}>
                      {p.label}
                    </button>
                  ))}
                </div>
              )}
            </Row>
          </div>
        )}
        {error && (
          <p role="alert" className="mt-2 text-xs" style={{ color: "var(--danger)" }}>
            {error}
          </p>
        )}
      </section>

      <section aria-label="Appearance" className="soft-card panel-in p-5">
        <h2 className="mb-2 text-lg font-medium">Appearance</h2>
        <div role="radiogroup" aria-label="Theme" className="flex flex-wrap gap-2">
          {THEMES.map((t) => (
            <label key={t.id} className="flex cursor-pointer items-center gap-2 rounded-full px-3 py-1.5 text-sm" style={{ background: "var(--surface-raised)", border: `1px solid ${theme === t.id ? "var(--accent)" : "var(--border)"}` }}>
              <input type="radio" name="theme" checked={theme === t.id} onChange={() => onTheme(t.id)} />
              {t.label}
            </label>
          ))}
        </div>
        <p className="mt-2 text-xs" style={muted}>
          Animations follow macOS: with Reduce Motion on, BOB stops floating.
        </p>
      </section>

      <section aria-label="Keyboard" className="soft-card panel-in p-5">
        <h2 className="mb-2 text-lg font-medium">Keyboard</h2>
        <ul className="grid gap-1 text-sm md:grid-cols-2">
          <li><kbd>Tab</kbd> / <kbd>⇧Tab</kbd> / <kbd>Esc</kbd>: take a word / take all / dismiss a suggestion</li>
          <li><kbd>⌘⇧J</kbd>: Selection Actions (change it in Selection Actions)</li>
          <li><kbd>⌥Space</kbd> (hold): Voice Dictation (change it in Voice Dictation)</li>
          <li><kbd>⌘1</kbd> to <kbd>⌘7</kbd>: switch panels · <kbd>⌘,</kbd>: Settings</li>
        </ul>
      </section>

      <section aria-label="Welcome tour" className="soft-card panel-in p-5">
        <h2 className="mb-2 text-lg font-medium">Welcome tour</h2>
        <p className="mb-3 text-sm" style={muted}>
          Permissions, models and a cheat sheet, step by step.
        </p>
        <button type="button" onClick={onTour} className="rounded-full px-4 py-1.5 text-sm font-medium" style={{ background: "var(--accent)", color: "var(--accent-fg)" }}>
          Show the welcome tour
        </button>
      </section>

      <section aria-label="About BOB" className="soft-card panel-in flex items-center gap-5 p-5">
        <div className="shrink-0">
          <GhostMark size={56} float />
        </div>
        <div>
          <h2 className="text-lg font-medium">BOB: Bring Your Own Brain</h2>
          <p className="text-sm" style={muted}>
            {build}. No accounts, no telemetry. Text goes only to the brain you choose; cloud brains get no personal context unless you allow
            it. Speech is recognized on this Mac and audio is never saved. Built with Kiro for Kiroween 2026 from llama.cpp, whisper.cpp, Tauri,
            Swift and React (see THIRD_PARTY_NOTICES.md). MIT licensed.
          </p>
        </div>
      </section>
    </div>
  );
}

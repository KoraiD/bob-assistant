import { useEffect, useState, type ReactNode } from "react";
import { BrainPanel } from "./components/BrainPanel";
import { DictationCard } from "./components/DictationCard";
import { GhostCard } from "./components/GhostCard";
import { isOnboarded, OnboardingWizard } from "./components/OnboardingWizard";
import { Orb } from "./components/Orb";
import { PermissionsCard } from "./components/PermissionsCard";
import { PersonalizePanel } from "./components/PersonalizePanel";
import { SelectionCard } from "./components/SelectionCard";
import { SettingsPanel } from "./components/SettingsPanel";
import { StatsPanel } from "./components/StatsPanel";
import { BROWSER_APP_INFO, getAppInfo, isTauri, type AppInfo } from "./lib/appInfo";
import { generalApi, OPEN_PANEL_EVENT, pauseLabel, SHELL_EVENT, type PauseView } from "./lib/general";
import { useOrb } from "./lib/orb";
import { useTauriEvent } from "./lib/useTauriEvent";
import { useTheme } from "./lib/theme";

/** The three capabilities, in priority order (see .kiro/steering/product.md). */
export const CAPABILITIES = [
  { id: "ghost", title: "Ghost Text", blurb: "Inline next-word predictions at your caret, in any app.", status: "Live", how: "Type, pause, press Tab" },
  { id: "selection", title: "Selection Actions", blurb: "Highlight text, press a hotkey, rewrite it in place.", status: "Live", how: "Select, press ⌘⇧J" },
  { id: "dictation", title: "Voice Dictation", blurb: "Hold a key, speak, release. Clean text appears at the caret.", status: "Live", how: "Hold ⌥Space, speak" },
] as const;

/** Footer text: "BOB 0.1.0 · macos aarch64", or a plain preview label outside the app. */
export function formatBuild(info: AppInfo): string {
  if (info.platform === "browser") return "Browser preview";
  return `BOB ${info.version} · ${info.platform} ${info.arch}`;
}

export type Panel = "overview" | "ghost" | "selection" | "dictation" | "brains" | "personalize" | "stats" | "settings";

/** UI-1: one panel per capability plus Brains, Personalize, Statistics and Settings. */
export const PANELS: { id: Panel; label: string; icon: string }[] = [
  { id: "overview", label: "Overview", icon: "🏚️" },
  { id: "ghost", label: "Ghost Text", icon: "👻" },
  { id: "selection", label: "Selection Actions", icon: "✨" },
  { id: "dictation", label: "Voice Dictation", icon: "🎙️" },
  { id: "brains", label: "Brains", icon: "🧠" },
  { id: "personalize", label: "Personalize", icon: "🪄" },
  { id: "stats", label: "Statistics", icon: "📈" },
  { id: "settings", label: "Settings", icon: "🕯️" },
];

function Overview({ go }: { go: (p: Panel) => void }) {
  return (
    <>
      <section aria-label="Capabilities" className="grid gap-4 md:grid-cols-3">
        {CAPABILITIES.map((c) => (
          <article key={c.id} className="soft-card panel-in flex flex-col p-5" data-testid={`capability-${c.id}`}>
            <h3 className="mb-1 text-lg font-medium">{c.title}</h3>
            <p className="mb-3 text-sm" style={{ color: "var(--text-muted)" }}>
              {c.blurb}
            </p>
            <p className="mb-4 text-xs" style={{ color: "var(--text-muted)" }}>
              <kbd>{c.how}</kbd>
            </p>
            <div className="mt-auto flex items-center justify-between">
              <span className="rounded-full px-3 py-1 text-xs" style={{ background: "var(--accent-2-soft)", color: "var(--accent-2)" }}>
                {c.status}
              </span>
              <button type="button" onClick={() => go(c.id)} className="rounded-full px-3 py-1 text-xs" style={{ background: "var(--surface-raised)", border: "1px solid var(--border-strong)" }} aria-label={`Open ${c.title}`}>
                Open
              </button>
            </div>
          </article>
        ))}
      </section>
      <PermissionsCard />
    </>
  );
}

export default function App() {
  const [info, setInfo] = useState<AppInfo>(BROWSER_APP_INFO);
  // `?panel=` opens a panel directly (scripts/screenshot.sh, demo recording).
  const [panel, setPanel] = useState<Panel>(() => {
    const p = new URLSearchParams(window.location.search).get("panel");
    return PANELS.some((x) => x.id === p) ? (p as Panel) : "overview";
  });
  const [theme, setTheme] = useTheme();
  const [tour, setTour] = useState(() => isTauri() && !isOnboarded());
  const orb = useOrb();
  const [pause, setPause] = useState<PauseView>({ paused: false, until: null });
  const refreshPause = () => {
    if (isTauri()) void generalApi.get().then((g) => setPause(g.pause)).catch(() => {});
  };
  useEffect(refreshPause, []);
  useTauriEvent(SHELL_EVENT, refreshPause);
  // Cmd+, from the app menu or "Settings…" in the menu bar.
  useTauriEvent<string>(OPEN_PANEL_EVENT, (p) => PANELS.some((x) => x.id === p) && setPanel(p as Panel));
  // Cmd+1 ... Cmd+8 switch panels, like tabs in Mac apps.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return;
      const n = Number(e.key);
      if (n >= 1 && n <= PANELS.length) {
        e.preventDefault();
        setPanel(PANELS[n - 1].id);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  useEffect(() => {
    let alive = true;
    getAppInfo().then((i) => alive && setInfo(i));
    return () => {
      alive = false;
    };
  }, []);

  const current = PANELS.find((p) => p.id === panel)!;
  const body: Record<Panel, ReactNode> = {
    overview: <Overview go={setPanel} />,
    ghost: <GhostCard />,
    selection: <SelectionCard />,
    dictation: <DictationCard />,
    brains: <BrainPanel />,
    personalize: <PersonalizePanel />,
    stats: <StatsPanel />,
    settings: <SettingsPanel build={formatBuild(info)} theme={theme} onTheme={setTheme} onTour={() => setTour(true)} />,
  };

  return (
    <div className="flex min-h-screen" style={{ background: "var(--bg)" }}>
      <aside className="flex w-60 shrink-0 flex-col gap-6 p-5" style={{ background: "var(--bg-sidebar)", borderRight: "1px solid var(--border)" }}>
        <header>
          <h1 className="text-3xl font-semibold tracking-tight">BOB</h1>
          <p className="text-xs" style={{ color: "var(--text-muted)" }}>
            Bring Your Own Brain
          </p>
        </header>
        <Orb view={orb} size={48} />
        {pause.paused && (
          <div className="rounded-xl p-3 text-xs" style={{ background: "var(--accent-2-soft)", color: "var(--accent-2)" }} data-testid="paused-banner">
            <p className="mb-2 font-medium">{pauseLabel(pause)}</p>
            <button
              type="button"
              onClick={() => void generalApi.resume().then(setPause)}
              className="rounded-full px-3 py-1 font-medium"
              style={{ background: "var(--accent)", color: "var(--accent-fg)" }}
            >
              Resume BOB
            </button>
          </div>
        )}
        <nav aria-label="Sections">
          <ul className="space-y-1">
            {PANELS.map((p) => (
              <li key={p.id}>
                <button
                  type="button"
                  aria-current={panel === p.id ? "page" : undefined}
                  onClick={() => setPanel(p.id)}
                  title={`${p.label} (⌘${PANELS.indexOf(p) + 1})`}
                  className="flex w-full items-center gap-3 rounded-xl px-3 py-2 text-left text-sm"
                  style={panel === p.id ? { background: "var(--accent-soft)", color: "var(--accent)" } : { color: "var(--text)" }}
                >
                  <span aria-hidden="true">{p.icon}</span>
                  {p.label}
                </button>
              </li>
            ))}
          </ul>
        </nav>
        <footer className="mt-auto text-xs" style={{ color: "var(--text-muted)" }}>
          {formatBuild(info)}
        </footer>
      </aside>
      <main className="min-w-0 flex-1 p-8" aria-label={current.label}>
        {panel === "overview" && (
          <div className="panel-in mb-6">
            <h2 className="text-2xl font-semibold">Welcome to the haunt</h2>
            <p className="text-sm" style={{ color: "var(--text-muted)" }}>
              A friendly ghost for every app. Here is what I can do:
            </p>
          </div>
        )}
        <div className="panel-body">{body[panel]}</div>
      </main>
      {tour && <OnboardingWizard onClose={() => setTour(false)} />}
    </div>
  );
}

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
import { Icon } from "./components/ui";

/** The three capabilities, in priority order (see .kiro/steering/product.md). */
export const CAPABILITIES = [
  { id: "ghost", title: "Ghost Text", blurb: "Suggests the next words where you type, in any app.", how: "Pause, then Tab" },
  { id: "selection", title: "Selection Actions", blurb: "Rewrite selected text in place: fix, shorten, translate.", how: "Select, then ⌘⇧J" },
  { id: "dictation", title: "Voice Dictation", blurb: "Speak and BOB types it at the cursor. Recognized on this Mac.", how: "Hold ⌥Space" },
] as const;

/** Footer text: "BOB 0.1.0 · macos aarch64", or a plain preview label outside the app. */
export function formatBuild(info: AppInfo): string {
  if (info.platform === "browser") return "Browser preview";
  return `BOB ${info.version} · ${info.platform} ${info.arch}`;
}

export type Panel = "overview" | "ghost" | "selection" | "dictation" | "brains" | "personalize" | "stats" | "settings";

/** UI-1: one panel per capability plus Brains, Personalize, Statistics and Settings. */
export const PANELS: { id: Panel; label: string }[] = [
  { id: "overview", label: "Overview" },
  { id: "ghost", label: "Ghost Text" },
  { id: "selection", label: "Selection Actions" },
  { id: "dictation", label: "Voice Dictation" },
  { id: "brains", label: "Brains" },
  { id: "personalize", label: "Personalize" },
  { id: "stats", label: "Statistics" },
  { id: "settings", label: "Settings" },
];

function Overview({ go }: { go: (p: Panel) => void }) {
  return (
    <>
      <section aria-label="Capabilities" className="mac-group-body panel-in">
        {CAPABILITIES.map((c) => (
          <article key={c.id} className="mac-row" data-testid={`capability-${c.id}`}>
            <span className="grid h-8 w-8 shrink-0 place-items-center rounded-lg" style={{ background: "var(--accent-soft)", color: "var(--accent)" }}>
              <Icon name={c.id} />
            </span>
            <div className="min-w-0 flex-1">
              <h3 className="mac-row-title font-medium">{c.title}</h3>
              <p className="mac-row-detail">
                {c.blurb} <kbd className="ml-1">{c.how}</kbd>
              </p>
            </div>
            <button type="button" onClick={() => go(c.id)} className="mac-button" aria-label={`Open ${c.title}`}>
              Open
            </button>
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
    <div className="relative flex min-h-screen">
      <aside className="sidebar flex w-56 shrink-0 flex-col gap-4 px-3 pb-4 pt-12" style={{ borderRight: "1px solid var(--border)" }}>
        {/* Space for the traffic lights; dragging here moves the window. */}
        <div data-tauri-drag-region className="absolute left-0 top-0 h-10 w-56" aria-hidden />
        <header className="px-2">
          <h1 className="text-xl font-semibold tracking-tight">BOB</h1>
          <p className="text-[11px]" style={{ color: "var(--text-muted)" }}>
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
          <ul className="space-y-0.5">
            {PANELS.map((p) => (
              <li key={p.id}>
                <button
                  type="button"
                  aria-current={panel === p.id ? "page" : undefined}
                  onClick={() => setPanel(p.id)}
                  title={`${p.label} (⌘${PANELS.indexOf(p) + 1})`}
                  className="nav-item"
                >
                  <span style={{ color: panel === p.id ? "var(--accent)" : "var(--text-muted)" }}>
                    <Icon name={p.id} size={16} />
                  </span>
                  {p.label}
                </button>
              </li>
            ))}
          </ul>
        </nav>
        <footer className="mt-auto px-2 text-[11px]" style={{ color: "var(--text-muted)" }}>
          {formatBuild(info)}
        </footer>
      </aside>
      <main className="relative min-w-0 flex-1 px-8 pb-8 pt-10" aria-label={current.label} style={{ background: "var(--bg)" }}>
        <div data-tauri-drag-region className="absolute left-0 right-0 top-0 h-10" aria-hidden />
        <h2 className="panel-in mb-4 text-[20px] font-semibold">{current.label}</h2>
        {panel === "overview" && (
          <div className="panel-in mb-6">
            <p style={{ color: "var(--text-muted)" }}>
              Three ways BOB helps you write, with the brain you choose.
            </p>
          </div>
        )}
        <div className="panel-body">{body[panel]}</div>
      </main>
      {tour && <OnboardingWizard onClose={() => setTour(false)} />}
    </div>
  );
}

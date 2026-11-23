import { useEffect, useState } from "react";
import { GhostMark } from "./components/GhostMark";
import { BrainPanel } from "./components/BrainPanel";
import { GhostCard } from "./components/GhostCard";
import { PermissionsCard } from "./components/PermissionsCard";
import { PersonalizePanel } from "./components/PersonalizePanel";
import { SelectionCard } from "./components/SelectionCard";
import { BROWSER_APP_INFO, getAppInfo, type AppInfo } from "./lib/appInfo";

/** The three capabilities, in priority order (see .kiro/steering/product.md). */
export const CAPABILITIES = [
  { id: "ghost", title: "Ghost Text", blurb: "Inline next-word predictions at your caret, in any app.", status: "Live" },
  { id: "selection", title: "Selection Actions", blurb: "Highlight text, press a hotkey, rewrite it in place.", status: "Live" },
  { id: "dictation", title: "Voice Dictation", blurb: "Hold a key, speak, release. Clean text appears at the caret.", status: "Coming soon" },
] as const;

/** Footer text: "BOB 0.1.0 · macos aarch64", or a plain preview label outside the app. */
export function formatBuild(info: AppInfo): string {
  if (info.platform === "browser") return "Browser preview";
  return `BOB ${info.version} · ${info.platform} ${info.arch}`;
}

type Tab = "overview" | "brains" | "personalize";
const TABS: { id: Tab; label: string }[] = [
  { id: "overview", label: "Overview" },
  { id: "brains", label: "Brains" },
  { id: "personalize", label: "Personalize" },
];

export default function App() {
  const [info, setInfo] = useState<AppInfo>(BROWSER_APP_INFO);
  const [tab, setTab] = useState<Tab>("overview");

  useEffect(() => {
    let alive = true;
    getAppInfo().then((i) => alive && setInfo(i));
    return () => {
      alive = false;
    };
  }, []);

  return (
    <main className="min-h-screen p-8" style={{ background: "var(--bg)" }}>
      <header className="panel-in mb-8 flex items-center gap-4">
        <GhostMark size={72} float />
        <div>
          <h1 className="text-3xl font-semibold tracking-tight">BOB</h1>
          <p style={{ color: "var(--text-muted)" }}>Bring Your Own Brain. A friendly ghost for every app.</p>
        </div>
      </header>

      <nav aria-label="Sections" className="mb-6 flex gap-2" role="tablist">
        {TABS.map((t) => (
          <button
            key={t.id}
            type="button"
            role="tab"
            aria-selected={tab === t.id}
            onClick={() => setTab(t.id)}
            className="rounded-full px-4 py-1.5 text-sm font-medium"
            style={
              tab === t.id
                ? { background: "var(--accent)", color: "var(--accent-fg)" }
                : { background: "var(--surface)", color: "var(--text-muted)", border: "1px solid var(--border)" }
            }
          >
            {t.label}
          </button>
        ))}
      </nav>

      {tab === "brains" ? (
        <BrainPanel />
      ) : tab === "personalize" ? (
        <PersonalizePanel />
      ) : (
        <>
      <section aria-label="Capabilities" className="grid gap-4 md:grid-cols-3">
        {CAPABILITIES.map((c) => (
          <article key={c.id} className="soft-card panel-in p-5" data-testid={`capability-${c.id}`}>
            <h2 className="mb-1 text-lg font-medium">{c.title}</h2>
            <p className="mb-3 text-sm" style={{ color: "var(--text-muted)" }}>
              {c.blurb}
            </p>
            <span
              className="rounded-full px-3 py-1 text-xs"
              style={{ background: "var(--accent-2-soft)", color: "var(--accent-2)" }}
            >
              {c.status}
            </span>
          </article>
        ))}
      </section>

      <GhostCard />
      <SelectionCard />
      <PermissionsCard />
        </>
      )}

      <footer className="mt-10 text-xs" style={{ color: "var(--text-muted)" }}>
        {formatBuild(info)}
      </footer>
    </main>
  );
}

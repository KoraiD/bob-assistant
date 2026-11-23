import { THEMES, type Theme } from "../lib/theme";
import { GhostMark } from "./GhostMark";

const muted = { color: "var(--text-muted)" } as const;

/** About: version, theme, the welcome tour, privacy in one paragraph, and credits. */
export function AboutPanel({ build, theme, onTheme, onTour }: { build: string; theme: Theme; onTheme: (t: Theme) => void; onTour: () => void }) {
  return (
    <div className="space-y-4">
      <section aria-label="About BOB" className="soft-card panel-in flex items-center gap-5 p-6">
        <GhostMark size={72} float />
        <div>
          <h2 className="text-xl font-semibold">BOB: Bring Your Own Brain</h2>
          <p className="text-sm" style={muted}>A friendly ghost for every app. {build}</p>
        </div>
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
        <p className="mt-2 text-xs" style={muted}>Animations follow macOS: with Reduce Motion on, BOB stops floating.</p>
      </section>

      <section aria-label="Welcome tour" className="soft-card panel-in p-5">
        <h2 className="mb-2 text-lg font-medium">Welcome tour</h2>
        <p className="mb-3 text-sm" style={muted}>Permissions, models and a cheat sheet, step by step.</p>
        <button type="button" onClick={onTour} className="rounded-full px-4 py-1.5 text-sm font-medium" style={{ background: "var(--accent)", color: "var(--accent-fg)" }}>
          Show the welcome tour
        </button>
      </section>

      <section aria-label="Privacy" className="soft-card panel-in p-5">
        <h2 className="mb-2 text-lg font-medium">Privacy in one paragraph</h2>
        <p className="text-sm" style={muted}>
          No accounts, no telemetry. Text goes only to the brain you choose; local brains stay on this Mac, cloud brains are labeled and get no
          personal context unless you allow it. Speech is recognized on this Mac and audio is never saved. Password fields are never read. Keys
          live in the macOS Keychain. Details: docs/PRIVACY.md.
        </p>
      </section>

      <section aria-label="Credits" className="soft-card panel-in p-5">
        <h2 className="mb-2 text-lg font-medium">Credits</h2>
        <p className="text-sm" style={muted}>
          Built with Kiro for Kiroween 2026. Stitched together from llama.cpp, whisper.cpp, Tauri, Swift and React, all open source (see
          THIRD_PARTY_NOTICES.md). MIT licensed.
        </p>
      </section>
    </div>
  );
}

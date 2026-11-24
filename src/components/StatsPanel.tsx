import { useCallback, useEffect, useState } from "react";
import { isTauri } from "../lib/appInfo";
import { api, type GhostStatus } from "../lib/brains";
import { dictationApi, type DictationStats } from "../lib/dictation";
import { acceptanceRate, minutesLabel, usageApi, weekday, wordsForYou, type Day, type UsageView } from "../lib/usage";

const muted = { color: "var(--text-muted)" } as const;
const field = { background: "var(--surface-raised)", border: "1px solid var(--border-strong)" } as const;

function Tile({ label, value, detail, testId }: { label: string; value: string; detail: string; testId: string }) {
  return (
    <div className="soft-card panel-in p-5" data-testid={testId}>
      <p className="text-xs" style={muted}>
        {label}
      </p>
      <p className="mt-1 text-3xl font-semibold tabular-nums">{value}</p>
      <p className="mt-1 text-xs" style={muted}>
        {detail}
      </p>
    </div>
  );
}

/** 14 stacked bars: Ghost Text words (ectoplasm) under dictated words (pumpkin). */
export function DayBars({ recent }: { recent: [string, Day][] }) {
  const W = 560;
  const H = 140;
  const pad = 18;
  const max = Math.max(1, ...recent.map(([, d]) => wordsForYou(d)));
  const slot = W / Math.max(1, recent.length);
  const bar = Math.min(26, slot * 0.6);
  const total = recent.reduce((n, [, d]) => n + wordsForYou(d), 0);
  return (
    <figure className="mt-2">
      <svg viewBox={`0 0 ${W} ${H + pad}`} className="w-full" role="img" aria-label={`Words BOB typed for you per day, last 14 days: ${total} in total.`}>
        <line x1="0" x2={W} y1={H} y2={H} stroke="var(--border-strong)" />
        {recent.map(([date, d], i) => {
          const g = (d.ghost_words / max) * (H - 8);
          const v = (d.dictated_words / max) * (H - 8);
          const x = i * slot + (slot - bar) / 2;
          return (
            <g key={date}>
              <title>{`${date}: ${d.ghost_words} from Ghost Text, ${d.dictated_words} dictated`}</title>
              {g > 0 && <rect x={x} y={H - g} width={bar} height={g} rx="3" fill="var(--accent)" />}
              {v > 0 && <rect x={x} y={H - g - v} width={bar} height={v} rx="3" fill="var(--accent-2)" />}
              <text x={x + bar / 2} y={H + 13} textAnchor="middle" fontSize="10" fill="var(--text-muted)">
                {weekday(date).slice(0, 2)}
              </text>
            </g>
          );
        })}
      </svg>
      <figcaption className="mt-1 flex gap-4 text-xs" style={muted}>
        <span>
          <span aria-hidden style={{ color: "var(--accent)" }}>■</span> Ghost Text
        </span>
        <span>
          <span aria-hidden style={{ color: "var(--accent-2)" }}>■</span> Dictation
        </span>
      </figcaption>
    </figure>
  );
}

/** Statistics: how much BOB wrote for you, per feature, from local counts only. */
export function StatsPanel() {
  const [usage, setUsage] = useState<UsageView | null>(null);
  const [ghost, setGhost] = useState<GhostStatus | null>(null);
  const [dict, setDict] = useState<DictationStats | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!isTauri()) return;
    try {
      setUsage(await usageApi.get());
      const [g, d] = await Promise.allSettled([api.ghostStatus(), dictationApi.get()]);
      if (g.status === "fulfilled") setGhost(g.value);
      if (d.status === "fulfilled") setDict(d.value.stats);
    } catch (e) {
      setError(String(e));
    }
  }, []);
  useEffect(() => {
    void refresh();
    const t = setInterval(() => void refresh(), 5000);
    return () => clearInterval(t);
  }, [refresh]);

  if (!isTauri()) {
    return (
      <p className="soft-card p-5 text-sm" style={muted}>
        Open the BOB app to see your statistics.
      </p>
    );
  }
  if (!usage) return <p style={muted}>Counting...</p>;

  const a = usage.all_time;
  const rate = acceptanceRate(a);
  const clear = async () => {
    try {
      await usageApi.clear();
      setConfirm(false);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };
  return (
    <div>
      {error && (
        <p role="alert" className="mb-2 text-sm" style={{ color: "var(--danger)" }}>
          {error}
        </p>
      )}
      <div className="grid gap-4 md:grid-cols-3">
        <Tile testId="stat-today" label="Today" value={`${wordsForYou(usage.today)}`} detail="words BOB typed for you" />
        <Tile testId="stat-week" label="Last 7 days" value={`${wordsForYou(usage.week)}`} detail={`about ${minutesLabel(usage.minutes_saved_week)} of typing saved`} />
        <Tile
          testId="stat-all"
          label={usage.since ? `Since ${usage.since}` : "All time"}
          value={`${wordsForYou(a)}`}
          detail={`about ${minutesLabel(usage.minutes_saved_all)} of typing saved`}
        />
      </div>

      <section aria-label="Last 14 days" className="soft-card panel-in mt-4 p-5">
        <h2 className="text-lg font-medium">Last 14 days</h2>
        <DayBars recent={usage.recent} />
      </section>

      <section aria-label="By feature" className="soft-card panel-in mt-4 p-5">
        <h2 className="mb-2 text-lg font-medium">By feature</h2>
        <dl className="grid gap-x-6 gap-y-3 text-sm md:grid-cols-3">
          <div data-testid="stat-ghost">
            <dt className="font-medium">Ghost Text</dt>
            <dd style={muted}>
              {a.ghost_words} words from {a.ghost_accepts} accepted suggestions
              {rate !== null && ` · ${rate}% of ${a.ghost_shown} shown`}
              {ghost?.p50_ms != null && ` · first suggestion in ${ghost.p50_ms} ms (median)`}
            </dd>
          </div>
          <div data-testid="stat-dictation">
            <dt className="font-medium">Voice Dictation</dt>
            <dd style={muted}>
              {a.dictated_words} words in {a.dictations} dictations · {minutesLabel(a.dictated_ms / 60000)} of speech
              {dict?.last_transcribe_ms != null && ` · last one transcribed in ${dict.last_transcribe_ms} ms`}
            </dd>
          </div>
          <div data-testid="stat-selection">
            <dt className="font-medium">Selection Actions</dt>
            <dd style={muted}>
              {a.rewrites} rewrites · {a.voice_edits} voice edits{a.rewrite_failures > 0 && ` · ${a.rewrite_failures} did not finish`}
            </dd>
          </div>
        </dl>
      </section>

      <div className="mt-4 flex flex-wrap items-center gap-3 text-xs" style={muted}>
        <p className="flex-1">
          Counts only, never your text, kept in usage.json on this Mac. Time saved assumes typing at 40 words per minute.
        </p>
        {confirm ? (
          <span className="flex items-center gap-2" role="group" aria-label="Confirm clearing statistics">
            Clear all statistics?
            <button type="button" onClick={() => void clear()} className="rounded-full px-3 py-1" style={{ ...field, color: "var(--danger)" }}>
              Clear
            </button>
            <button type="button" onClick={() => setConfirm(false)} className="rounded-full px-3 py-1" style={field}>
              Keep
            </button>
          </span>
        ) : (
          <button type="button" onClick={() => setConfirm(true)} className="rounded-full px-3 py-1" style={field}>
            Clear statistics…
          </button>
        )}
      </div>
    </div>
  );
}

import { invoke } from "@tauri-apps/api/core";
// Mirrors src-tauri/src/usage.rs and system.rs.
export interface Day {
  ghost_words: number;
  ghost_accepts: number;
  ghost_shown: number;
  dictated_words: number;
  dictations: number;
  dictated_ms: number;
  rewrites: number;
  rewrite_failures: number;
  voice_edits: number;
  typo_fixes: number;
}
export interface UsageView {
  today: Day;
  week: Day;
  all_time: Day;
  /** The last 14 days, oldest first: [date "YYYY-MM-DD", counts]. */
  recent: [string, Day][];
  minutes_saved_week: number;
  minutes_saved_all: number;
  since: string | null;
}
export interface Recommendation {
  brain_models: string[];
  speech_model: string;
  note: string;
}
export interface SystemInfo {
  chip: string;
  ram_gb: number;
  apple_silicon: boolean;
  recommendation: Recommendation;
}

export const wordsForYou = (d: Day) => d.ghost_words + d.dictated_words;

/** Share of shown suggestions that were accepted, as a whole percent; null before any were shown. */
export function acceptanceRate(d: Day): number | null {
  if (d.ghost_shown === 0) return null;
  return Math.round((Math.min(d.ghost_accepts, d.ghost_shown) / d.ghost_shown) * 100);
}

/** "12 min", "1 h 5 min", "less than a minute". */
export function minutesLabel(min: number): string {
  if (min < 1) return "less than a minute";
  const m = Math.round(min);
  if (m < 60) return `${m} min`;
  const h = Math.floor(m / 60);
  return m % 60 === 0 ? `${h} h` : `${h} h ${m % 60} min`;
}

/** Short weekday for a "YYYY-MM-DD" date in the local calendar ("Mon"). */
export function weekday(date: string): string {
  const [y, m, d] = date.split("-").map(Number);
  return new Date(y, m - 1, d).toLocaleDateString([], { weekday: "short" });
}

export const usageApi = {
  get: () => invoke<UsageView>("usage_get"),
  clear: () => invoke<void>("usage_clear"),
  system: () => invoke<SystemInfo>("system_info"),
};

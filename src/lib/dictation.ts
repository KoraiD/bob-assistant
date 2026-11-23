import { invoke } from "@tauri-apps/api/core";
import type { ModelInfo } from "./brains";
import type { HotkeySetting } from "./selection";

// Mirrors src-tauri/src/dictation/{settings,engine}.rs and commands.rs.
export type Cleanup = "none" | "clean" | "translate_english" | "meeting_notes";

export interface DictationSettings {
  enabled: boolean;
  model: string;
  language: string;
  hotkey: HotkeySetting;
  cleanup: Cleanup;
}

export interface DictationStats {
  dictations: number;
  discarded: number;
  errors: number;
  last_error: string | null;
  last_audio_ms: number | null;
  last_transcribe_ms: number | null;
  last_release_to_insert_ms: number | null;
  model_loaded: boolean;
  model_error: string | null;
}

export interface DictationView {
  settings: DictationSettings;
  models: ModelInfo[];
  stats: DictationStats;
  microphone: "granted" | "denied" | "undetermined" | null;
}

/** Hold-to-talk choices (key codes: Space = 49, D = 2). None overlaps the Selection Actions presets. */
export const DICTATION_HOTKEYS: { id: string; label: string; hotkey: HotkeySetting }[] = [
  { id: "opt-space", label: "⌥Space", hotkey: { key_code: 49, modifiers: ["option"] } },
  { id: "cmd-shift-space", label: "⌘⇧Space", hotkey: { key_code: 49, modifiers: ["cmd", "shift"] } },
  { id: "ctrl-opt-d", label: "⌃⌥D", hotkey: { key_code: 2, modifiers: ["control", "option"] } },
];

export const LANGUAGES: { code: string; label: string }[] = [
  { code: "", label: "Detect automatically" },
  { code: "en", label: "English" },
  { code: "hu", label: "Hungarian" },
  { code: "de", label: "German" },
  { code: "fr", label: "French" },
  { code: "es", label: "Spanish" },
  { code: "it", label: "Italian" },
  { code: "pt", label: "Portuguese" },
  { code: "nl", label: "Dutch" },
  { code: "pl", label: "Polish" },
  { code: "ro", label: "Romanian" },
  { code: "ja", label: "Japanese" },
  { code: "zh", label: "Chinese" },
];

export const CLEANUPS: { id: Cleanup; label: string }[] = [
  { id: "none", label: "None (fastest)" },
  { id: "clean", label: "Clean up fillers and punctuation" },
  { id: "translate_english", label: "Translate to English" },
  { id: "meeting_notes", label: "Meeting notes" },
];

/** Short status line for the card. */
export function dictationSummary(v: DictationView, hotkeyLabel: string): string {
  if (!v.settings.enabled) return "Off";
  const model = v.models.find((m) => m.id === v.settings.model);
  if (!model?.installed) return "Download the speech model below to start";
  if (v.microphone === "denied") return "Allow the microphone in System Settings > Privacy & Security > Microphone";
  if (v.stats.model_error) return `The speech model did not load: ${v.stats.model_error}`;
  if (!v.stats.model_loaded) return "Loading the speech model...";
  return `Hold ${hotkeyLabel}, speak, release. Text appears at your cursor.`;
}

export const dictationApi = {
  get: () => invoke<DictationView>("dictation_get"),
  setSettings: (settings: DictationSettings) => invoke<void>("dictation_set_settings", { settings }),
};

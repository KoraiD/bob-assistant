import { invoke } from "@tauri-apps/api/core";

// Mirrors src-tauri/src/context/mod.rs and commands.rs (serde snake_case).
export type ContextScope = "accessibility" | "active_window" | "all_screens";
export type Tone = "natural" | "friendly" | "professional" | "concise";
export type Quotes = "keep" | "straight" | "curly";
export type Spelling = "any" | "us" | "uk";
export const QUOTES: { id: Quotes; label: string }[] = [
  { id: "keep", label: "As written" },
  { id: "straight", label: "Straight \"quotes\" and 'apostrophes'" },
  { id: "curly", label: "Curly “quotes” and ’apostrophes’" },
];
export const SPELLINGS: { id: Spelling; label: string }[] = [
  { id: "any", label: "No preference" },
  { id: "us", label: "American (color, organize)" },
  { id: "uk", label: "British (colour, organise)" },
];

export interface GhostLayers {
  profile: boolean;
  memory: boolean;
  window: boolean;
  screen: boolean;
  /** "I write in Hungarian and English" before the text. */
  language_hint: boolean;
  /** Text copied in the last two minutes. */
  clipboard: boolean;
}
export interface AppMode {
  bundle_id: string;
  name: string;
  tone: Tone | null;
  /** A code from LANGUAGES, or "" for the global rule. */
  language: string;
  instructions: string;
}
/** Mirrors context/languages.rs (codes and names). */
export const LANGUAGES: { code: string; name: string }[] = [
  ["en", "English"], ["hu", "Hungarian"], ["de", "German"], ["fr", "French"], ["es", "Spanish"], ["it", "Italian"],
  ["pt", "Portuguese"], ["nl", "Dutch"], ["pl", "Polish"], ["ro", "Romanian"], ["cs", "Czech"], ["sk", "Slovak"],
  ["sv", "Swedish"], ["da", "Danish"], ["no", "Norwegian"], ["fi", "Finnish"], ["tr", "Turkish"], ["vi", "Vietnamese"],
  ["ru", "Russian"], ["uk", "Ukrainian"], ["el", "Greek"], ["ja", "Japanese"], ["zh", "Chinese"],
].map(([code, name]) => ({ code, name }));
export const MAX_PREFERRED = 4;
export const MAX_MODE_INSTRUCTIONS = 500;
export const MAX_STYLE_NOTES = 300;
export const languageName = (code: string) => LANGUAGES.find((l) => l.code === code)?.name ?? code;

export interface ContextSettings {
  scope: ContextScope;
  tone: Tone;
  language: string;
  ghost: GhostLayers;
  quotes: Quotes;
  spelling: Spelling;
  languages: string[];
  style_notes: string;
  app_modes: AppMode[];
}

export interface VisibleSummary {
  app_name: string;
  has_window_title: boolean;
  after_caret_chars: number;
  screen_chars: number;
  screen_ms: number | null;
  screen_age_s: number | null;
}

export interface ContextView {
  settings: ContextSettings;
  profile: string;
  profile_path: string | null;
  template: string;
  max_profile_bytes: number;
  visible: VisibleSummary;
  screen_recording: boolean | null;
}

export interface ContextPreview {
  ghost_notes: string;
  chat_system: string;
}

export const SCOPES: { id: ContextScope; label: string; detail: string }[] = [
  {
    id: "accessibility",
    label: "Accessibility only (recommended)",
    detail: "The window title and the text around your cursor. No screen capture, no extra permission.",
  },
  {
    id: "active_window",
    label: "Active window",
    detail: "Also reads the text visible in the window you are typing in (an email thread, a document). Needs Screen Recording; text is recognized on this Mac.",
  },
  {
    id: "all_screens",
    label: "All screens",
    detail: "Also reads the text on every display, for example an article on your second screen. Needs Screen Recording; quiet apps and BOB are never captured.",
  },
];

export const TONES: { id: Tone; label: string }[] = [
  { id: "natural", label: "Match my writing" },
  { id: "friendly", label: "Friendly" },
  { id: "professional", label: "Professional" },
  { id: "concise", label: "Concise" },
];

/**
 * Ghost Text layers the app offers (owner decision after the benchmark): remembered phrases alone, optionally
 * with the window title. The window title needs memory; profile and screen text stay benchmark-only.
 */
export function ghostLayers(
  memory: boolean,
  window: boolean,
  extra: Pick<GhostLayers, "language_hint" | "clipboard"> & { screen?: boolean } = { language_hint: false, clipboard: false },
): GhostLayers {
  return { profile: false, memory, window: memory && window, screen: extra.screen ?? false, language_hint: extra.language_hint, clipboard: extra.clipboard };
}

export const contextApi = {
  get: () => invoke<ContextView>("context_get"),
  setSettings: (settings: ContextSettings) => invoke<void>("context_set_settings", { settings }),
  saveProfile: (text: string) => invoke<void>("profile_save", { text }),
  requestScreenRecording: () => invoke<void>("context_request_screen_recording"),
  preview: () => invoke<ContextPreview>("context_preview"),
};

/** UTF-8 size, the unit of the profile limit. */
export function byteLength(s: string): number {
  return new TextEncoder().encode(s).length;
}

export type Format = "bold" | "italic" | "heading" | "bullet" | "task" | "quote" | "code" | "link";

export interface Edit {
  text: string;
  start: number;
  end: number;
}

/**
 * Apply a Markdown format to the selection [start, end) of `text`. Inline formats wrap the selection
 * (or insert a placeholder); line formats prefix every selected line. Returns the new text and selection.
 */
export function applyFormat(text: string, start: number, end: number, format: Format): Edit {
  const s = Math.max(0, Math.min(start, end, text.length));
  const e = Math.min(text.length, Math.max(start, end));
  const selected = text.slice(s, e);
  const wrap = (left: string, right: string, placeholder: string): Edit => {
    const inner = selected || placeholder;
    const out = text.slice(0, s) + left + inner + right + text.slice(e);
    return { text: out, start: s + left.length, end: s + left.length + inner.length };
  };
  switch (format) {
    case "bold":
      return wrap("**", "**", "bold text");
    case "italic":
      return wrap("_", "_", "italic text");
    case "code":
      return wrap("`", "`", "code");
    case "link": {
      const label = selected || "link text";
      const out = `${text.slice(0, s)}[${label}](https://)${text.slice(e)}`;
      const urlStart = s + label.length + 3;
      return { text: out, start: urlStart, end: urlStart + "https://".length };
    }
    default: {
      const prefix = { heading: "## ", bullet: "- ", task: "- [ ] ", quote: "> " }[format];
      const lineStart = text.lastIndexOf("\n", s - 1) + 1;
      const block = text.slice(lineStart, e);
      const prefixed = block
        .split("\n")
        .map((l) => (l.startsWith(prefix) ? l : prefix + l))
        .join("\n");
      const out = text.slice(0, lineStart) + prefixed + text.slice(e);
      return { text: out, start: lineStart, end: lineStart + prefixed.length };
    }
  }
}

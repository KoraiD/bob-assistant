import { invoke } from "@tauri-apps/api/core";

// Mirrors src-tauri/src/selection/{actions,engine}.rs.
export interface HotkeySetting {
  key_code: number;
  modifiers: string[];
}

export interface CustomAction {
  label: string;
  instruction: string;
  /** Runs this action directly, without the menu. Absent or null: no shortcut. */
  hotkey?: HotkeySetting | null;
}

export interface SelectionSettings {
  hotkey: HotkeySetting;
  custom: CustomAction[];
  /** Show the result first; Enter replaces, Esc discards. */
  preview: boolean;
}

export interface SelectionStats {
  menus: number;
  runs: number;
  replaced: number;
  failed: number;
  last_error: string | null;
  last_ms: number | null;
}

export interface SelectionView {
  settings: SelectionSettings;
  actions: string[];
  stats: SelectionStats;
}

export const MAX_CUSTOM = 12;

/** Hotkey choices (macOS virtual key codes: J = 38, Space = 49, E = 14). */
export const HOTKEYS: { id: string; label: string; hotkey: HotkeySetting }[] = [
  { id: "cmd-shift-j", label: "⌘⇧J", hotkey: { key_code: 38, modifiers: ["cmd", "shift"] } },
  { id: "ctrl-opt-space", label: "⌃⌥Space", hotkey: { key_code: 49, modifiers: ["control", "option"] } },
  { id: "cmd-opt-j", label: "⌘⌥J", hotkey: { key_code: 38, modifiers: ["cmd", "option"] } },
  { id: "ctrl-shift-space", label: "⌃⇧Space", hotkey: { key_code: 49, modifiers: ["control", "shift"] } },
  { id: "cmd-shift-e", label: "⌘⇧E", hotkey: { key_code: 14, modifiers: ["cmd", "shift"] } },
];

/** Shortcut choices for custom actions: ⌃⌥1 to ⌃⌥9 (key codes of the number row, the same on every layout). */
const DIGIT_CODES = [18, 19, 20, 21, 23, 22, 26, 28, 25];
export const ACTION_HOTKEYS: { id: string; label: string; hotkey: HotkeySetting }[] = DIGIT_CODES.map((key_code, i) => ({
  id: `ctrl-opt-${i + 1}`,
  label: `⌃⌥${i + 1}`,
  hotkey: { key_code, modifiers: ["control", "option"] },
}));
/** The shortcut choice matching `h`, or undefined (none, or set some other way). */
export function actionHotkeyPreset(h: HotkeySetting | null | undefined): (typeof ACTION_HOTKEYS)[number] | undefined {
  if (!h) return undefined;
  const key = (x: HotkeySetting) => `${x.key_code}:${[...x.modifiers].map((m) => m.toLowerCase()).sort().join("+")}`;
  return ACTION_HOTKEYS.find((p) => key(p.hotkey) === key(h));
}
/** The preset matching `h` (modifier order does not matter), or undefined for a custom one. */
export function hotkeyPreset(h: HotkeySetting): (typeof HOTKEYS)[number] | undefined {
  const key = (x: HotkeySetting) => `${x.key_code}:${[...x.modifiers].map((m) => m.toLowerCase()).sort().join("+")}`;
  return HOTKEYS.find((p) => key(p.hotkey) === key(h));
}

/** Custom actions that will be kept on save (both fields filled), mirroring the Rust sanitizer. */
export function validCustom(list: CustomAction[]): CustomAction[] {
  return list
    .map((c) => ({
      label: c.label.trim().replace(/\s+/g, " ").slice(0, 40),
      instruction: c.instruction.trim().slice(0, 1000),
      hotkey: c.hotkey ?? null,
    }))
    .filter((c) => c.label && c.instruction)
    .slice(0, MAX_CUSTOM);
}

export const selectionApi = {
  get: () => invoke<SelectionView>("selection_get"),
  setSettings: (settings: SelectionSettings) => invoke<void>("selection_set_settings", { settings }),
};

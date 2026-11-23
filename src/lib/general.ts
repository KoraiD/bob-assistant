import { invoke } from "@tauri-apps/api/core";

// Mirrors src-tauri/src/shell.rs, app_settings.rs and pause.rs.
export type LoginItem = "enabled" | "disabled" | "requires_approval" | "unavailable";
export type PauseHow = "hour" | "tomorrow" | "indefinite";

export interface PauseView {
  paused: boolean;
  /** Unix seconds; null when indefinite or not paused. */
  until: number | null;
}

export interface GeneralView {
  show_in_dock: boolean;
  open_window_at_launch: boolean;
  login_item: LoginItem;
  pause: PauseView;
}

export const SHELL_EVENT = "shell-changed";
export const OPEN_PANEL_EVENT = "open-panel";

export const generalApi = {
  get: () => invoke<GeneralView>("general_get"),
  setDock: (show: boolean) => invoke<void>("general_set_dock", { show }),
  setOpenAtLaunch: (open: boolean) => invoke<void>("general_set_open_at_launch", { open }),
  setLogin: (on: boolean) => invoke<LoginItem>("general_set_login", { on }),
  pause: (how: PauseHow) => invoke<PauseView>("pause_start", { how }),
  resume: () => invoke<PauseView>("pause_resume"),
};

/** "Paused until 14:30", "Paused until tomorrow, 08:00", or "Paused" (indefinite). */
export function pauseLabel(p: PauseView, now: Date = new Date()): string {
  if (!p.paused) return "Running";
  if (p.until === null) return "Paused";
  const t = new Date(p.until * 1000);
  const time = t.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const sameDay = t.toDateString() === now.toDateString();
  return sameDay ? `Paused until ${time}` : `Paused until tomorrow, ${time}`;
}

export function loginLabel(l: LoginItem): string {
  switch (l) {
    case "enabled":
      return "BOB starts when you log in.";
    case "disabled":
      return "BOB starts only when you open it.";
    case "requires_approval":
      return "Allow BOB in System Settings > General > Login Items.";
    case "unavailable":
      return "Available in the installed app (not in a development build).";
  }
}

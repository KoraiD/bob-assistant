import { invoke } from "@tauri-apps/api/core";

// Mirrors src-tauri/src/shell.rs, app_settings.rs and pause.rs.
export type LoginItem = "enabled" | "disabled" | "requires_approval" | "unavailable";
export type PauseHow = "hour" | "tomorrow" | "indefinite";

export interface PauseView {
  paused: boolean;
  /** Unix seconds; null when indefinite or not paused. */
  until: number | null;
}

export type LogLevel = "error" | "warn" | "info" | "debug" | "trace";
export const LOG_LEVELS: { id: LogLevel; label: string }[] = [
  { id: "error", label: "Errors only" },
  { id: "warn", label: "Warnings" },
  { id: "info", label: "Normal" },
  { id: "debug", label: "Detailed" },
  { id: "trace", label: "Everything (troubleshooting)" },
];
export interface GeneralView {
  show_in_dock: boolean;
  open_window_at_launch: boolean;
  login_item: LoginItem;
  pause: PauseView;
  log_level: LogLevel;
  log_dir: string | null;
  mcp_enabled: boolean;
  mcp_command: string | null;
}
export interface LogLine {
  at: number;
  level: string;
  target: string;
  message: string;
}
export interface AuditEntry {
  at: number;
  kind: string;
  detail: Record<string, unknown>;
}
/** The Kiro / MCP client configuration for BOB. */
export function mcpSnippet(command: string | null): string {
  return JSON.stringify({ mcpServers: { bob: { command: command ?? "/Applications/BOB.app/Contents/MacOS/bob-mcp" } } }, null, 2);
}
/** One line for an audit entry: "Brain removed: local-llama", "Text sent to cloud brain openai (420 characters)". */
export function auditLabel(e: AuditEntry): string {
  const d = e.detail as Record<string, unknown>;
  const str = (k: string) => (d[k] === undefined || d[k] === null ? "" : String(d[k]));
  switch (e.kind) {
    case "data.sent":
      return `Text sent to ${d.cloud ? "cloud" : "local"} brain ${str("brain")} for ${str("feature").replace("_", " ")} (${str("chars")} characters${d.personal_context ? ", with personal context" : ""})`;
    case "dictation_cleanup.withheld":
      return `Dictation not sent to brain ${str("brain")}: it looked like it contained a password or key`;
    case "model.mismatch":
      return "A model file failed its check and was deleted (download it again)";
    case "mcp.call":
      return `AI agent used ${str("tool")}${d.ok === false ? " (failed)" : ""}`;
    case "brain.saved":
      return `Brain saved: ${str("brain")}${d.key && d.key !== "unchanged" ? ` (API key ${str("key")})` : ""}`;
    case "brain.removed":
      return `Brain removed: ${str("brain")}`;
    case "brain.role":
      return `${str("role")} now uses ${str("brain") || "nothing"}`;
    case "brain.share_context":
      return `Personal context ${d.share ? "allowed" : "blocked"} for ${str("brain")}`;
    case "app.start":
      return `BOB started (${str("version")})`;
    case "app.exit":
      return "BOB quit";
    case "app.paused":
      return "BOB paused";
    case "app.resumed":
      return "BOB resumed";
    case "security.peer_rejected":
      return "Blocked a connection from another user";
    default: {
      const changed = d.changed as Record<string, unknown> | undefined;
      const what = changed ? Object.keys(changed).join(", ") : Object.entries(d).map(([k, v]) => `${k} ${String(v)}`).join(", ");
      return `${e.kind.replace(/[._]/g, " ")}${what ? `: ${what}` : ""}`;
    }
  }
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
  setLogLevel: (level: LogLevel) => invoke<void>("general_set_log_level", { level }),
  setMcp: (on: boolean) => invoke<void>("mcp_set_enabled", { on }),
  logs: (level: LogLevel) => invoke<LogLine[]>("logs_tail", { level }),
  audit: (prefix: string | null, limit = 200) => invoke<AuditEntry[]>("audit_recent", { prefix, limit }),
  openLogs: () => invoke<void>("logs_open_folder"),
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

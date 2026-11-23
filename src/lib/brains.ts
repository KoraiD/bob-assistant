import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isTauri } from "./appInfo";

// Mirrors src-tauri/src/brain/*.rs (serde snake_case).
export type ProviderKind = "builtin" | "ollama" | "lm_studio" | "open_ai_compatible";
export type Role = "ghost" | "selection" | "dictation";

export interface Capabilities {
  ghost: boolean;
  chat: boolean;
}

export interface BrainConfig {
  id: string;
  name: string;
  kind: ProviderKind;
  endpoint: string;
  model: string | null;
  capabilities: Capabilities;
  has_api_key: boolean;
}

export interface BrainView extends BrainConfig {
  is_cloud: boolean;
}

export type SidecarState =
  | { state: "stopped" }
  | { state: "starting" }
  | { state: "ready"; base_url: string }
  | { state: "failed"; message: string };

export interface Snapshot {
  brains: BrainView[];
  roles: Record<Role, string | null>;
  builtin: SidecarState;
  builtin_model: string | null;
}

export interface ModelInfo {
  id: string;
  name: string;
  role: string;
  file: string;
  size: number;
  license: string;
  min_ram_gb: number;
  installed: boolean;
  active: boolean;
}

export interface TestReport {
  reachable: boolean;
  latency_ms: number | null;
  models: string[];
  capabilities: Capabilities;
  is_cloud: boolean;
  error: string | null;
}

export interface TokenEvent {
  request_id: number;
  text: string | null;
  first_token_ms: number | null;
  done: boolean;
  error: string | null;
  /** Ghost mode, final event: what Ghost Text would show after polishing. */
  polished: string | null;
}

export interface GhostStatus {
  enabled: boolean;
  p50_ms: number | null;
  p95_ms: number | null;
  stats: {
    requests: number;
    chips_shown: number;
    accepted_words: number;
    accepted_all: number;
    dismissed: number;
    errors: number;
    last_error: string | null;
    typing_through: number;
  };
}

/** One-line summary of Ghost Text health for the Overview card. */
export function ghostSummary(g: GhostStatus | null, ghostBrain: string | null, builtinAwake: boolean): string {
  if (!g) return "Checking...";
  if (!g.enabled) return "Off";
  if (!ghostBrain) return "Choose a brain for Ghost Text in Brains";
  if (ghostBrain === "builtin" && !builtinAwake) return "Waiting for BOB's own brain to wake up";
  if (g.p50_ms !== null) return `Haunting your caret · p50 ${g.p50_ms} ms · p95 ${g.p95_ms} ms`;
  return "Ready. Pause while typing in any app";
}

export interface ProgressEvent {
  id: string;
  downloaded: number;
  total: number;
  done: boolean;
  error: string | null;
}

export const ROLES: { id: Role; label: string; needs: keyof Capabilities }[] = [
  { id: "ghost", label: "Ghost Text", needs: "ghost" },
  { id: "selection", label: "Selection Actions", needs: "chat" },
  { id: "dictation", label: "Dictation cleanup", needs: "chat" },
];

export const PROVIDERS: { kind: Exclude<ProviderKind, "builtin">; label: string; endpoint: string }[] = [
  { kind: "ollama", label: "Ollama", endpoint: "http://127.0.0.1:11434" },
  { kind: "lm_studio", label: "LM Studio", endpoint: "http://127.0.0.1:1234" },
  { kind: "open_ai_compatible", label: "OpenAI-compatible (local or cloud)", endpoint: "https://api.openai.com" },
];

// ---- pure helpers (tested) ----

export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n < 0) return "?";
  const units = ["B", "KB", "MB", "GB"];
  let v = n;
  let i = 0;
  while (v >= 1000 && i < units.length - 1) {
    v /= 1000;
    i++;
  }
  return `${i === 0 ? v : v.toFixed(1)} ${units[i]}`;
}

export function percent(done: number, total: number): number {
  if (total <= 0) return 0;
  return Math.max(0, Math.min(100, Math.round((done / total) * 100)));
}

/** Brains that may serve a role (the Rust registry enforces the same rule). */
export function brainsForRole(brains: BrainView[], role: Role): BrainView[] {
  const needs = ROLES.find((r) => r.id === role)!.needs;
  return brains.filter((b) => b.capabilities[needs]);
}

/** A stable id from a display name: "My Ollama!" -> "my-ollama". */
export function slugify(name: string): string {
  return (
    name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 48) || "brain"
  );
}

/** Make `base` unique among `taken` by appending -2, -3, ... */
export function uniqueId(base: string, taken: string[]): string {
  if (!taken.includes(base)) return base;
  for (let i = 2; ; i++) if (!taken.includes(`${base}-${i}`)) return `${base}-${i}`;
}

export interface Draft {
  name: string;
  kind: Exclude<ProviderKind, "builtin">;
  endpoint: string;
  model: string;
  apiKey: string;
}

/** Validation messages for the add-brain form; empty when the draft can be saved. */
export function validateDraft(d: Draft): string[] {
  const errors: string[] = [];
  if (!d.name.trim()) errors.push("Give the brain a name.");
  try {
    const u = new URL(d.endpoint);
    if (u.protocol !== "http:" && u.protocol !== "https:") errors.push("The endpoint must start with http:// or https://.");
  } catch {
    errors.push("The endpoint is not a valid URL.");
  }
  if (d.kind === "open_ai_compatible" && isCloudUrl(d.endpoint) && !d.apiKey.trim()) {
    errors.push("Cloud endpoints need an API key.");
  }
  return errors;
}

/** Same rule as `is_cloud_endpoint` in Rust: loopback hosts are local, anything else is cloud. */
export function isCloudUrl(endpoint: string): boolean {
  try {
    const h = new URL(endpoint).hostname.replace(/^\[|\]$/g, "");
    return !(h === "localhost" || h === "::1" || /^127\.\d+\.\d+\.\d+$/.test(h));
  } catch {
    return true;
  }
}

export function draftToConfig(d: Draft, takenIds: string[]): BrainConfig {
  const kindCaps: Record<Draft["kind"], Capabilities> = {
    ollama: { ghost: true, chat: true },
    lm_studio: { ghost: true, chat: true },
    open_ai_compatible: { ghost: false, chat: true },
  };
  return {
    id: uniqueId(slugify(d.name), takenIds),
    name: d.name.trim(),
    kind: d.kind,
    endpoint: d.endpoint.trim(),
    model: d.model.trim() || null,
    capabilities: kindCaps[d.kind],
    has_api_key: false,
  };
}

export function sidecarLabel(s: SidecarState): string {
  switch (s.state) {
    case "stopped":
      return "Sleeping";
    case "starting":
      return "Waking up...";
    case "ready":
      return "Awake";
    case "failed":
      return "Could not start";
  }
}

// ---- IPC ----

export const api = {
  snapshot: () => invoke<Snapshot>("brain_snapshot"),
  upsert: (config: BrainConfig, apiKey: string | null) => invoke<void>("brain_upsert", { config, apiKey }),
  remove: (id: string) => invoke<void>("brain_remove", { id }),
  setRole: (role: Role, id: string | null) => invoke<void>("brain_set_role", { role, id }),
  test: (id: string) => invoke<TestReport>("brain_test", { id }),
  tryIt: (id: string, prompt: string, mode: "ghost" | "chat") => invoke<number>("brain_try", { id, prompt, mode }),
  models: () => invoke<ModelInfo[]>("models_list"),
  download: (id: string) => invoke<void>("model_download", { id }),
  cancel: (id: string) => invoke<void>("model_cancel", { id }),
  startBuiltin: (modelId: string) => invoke<void>("builtin_start", { modelId }),
  stopBuiltin: () => invoke<void>("builtin_stop"),
  ghostStatus: () => invoke<GhostStatus>("ghost_status"),
  ghostSetEnabled: (enabled: boolean) => invoke<void>("ghost_set_enabled", { enabled }),
};

export async function on<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
  if (!isTauri()) return () => {};
  return listen<T>(event, (e) => handler(e.payload));
}

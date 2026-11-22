import { invoke } from "@tauri-apps/api/core";

/** Shape returned by the Rust `app_info` command (see src-tauri/src/app_info.rs). */
export interface AppInfo {
  name: string;
  version: string;
  platform: string;
  arch: string;
}

/** Fallback used when running in a plain browser (tests, `vite` without Tauri). */
export const BROWSER_APP_INFO: AppInfo = {
  name: "BOB",
  version: "dev",
  platform: "browser",
  arch: "n/a",
};

/** True when the Tauri IPC bridge is available. */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** Fetch app metadata from the Rust core, degrading gracefully outside Tauri. */
export async function getAppInfo(): Promise<AppInfo> {
  if (!isTauri()) return BROWSER_APP_INFO;
  try {
    return await invoke<AppInfo>("app_info");
  } catch {
    return BROWSER_APP_INFO;
  }
}

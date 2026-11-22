import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isTauri } from "./appInfo";

export type MicStatus = "granted" | "denied" | "undetermined";

/** Mirrors `BridgeStatus` in src-tauri/src/native_ipc.rs. */
export interface NativeStatus {
  connected: boolean;
  accessibility: boolean | null;
  microphone: MicStatus | null;
}

export const DISCONNECTED: NativeStatus = { connected: false, accessibility: null, microphone: null };

/** Name of the Tauri event emitted by the core when the status changes. */
export const STATUS_EVENT = "native-status";

export async function getNativeStatus(): Promise<NativeStatus> {
  if (!isTauri()) return DISCONNECTED;
  try {
    return await invoke<NativeStatus>("native_status");
  } catch {
    return DISCONNECTED;
  }
}

/** Subscribe to status changes. Returns an unsubscribe function. */
export async function onNativeStatus(handler: (s: NativeStatus) => void): Promise<() => void> {
  if (!isTauri()) return () => {};
  return listen<NativeStatus>(STATUS_EVENT, (e) => handler(e.payload));
}

/** Ask the helper to show the macOS permission prompts. Rejects with a readable message. */
export async function requestPermissions(): Promise<void> {
  if (!isTauri()) throw new Error("Permissions can only be requested in the BOB app.");
  await invoke("request_permissions");
}

export type PermissionState = "ok" | "needed" | "unknown" | "offline";

/** Presentation logic kept pure so it is easy to test. */
export function accessibilityState(s: NativeStatus): PermissionState {
  if (!s.connected) return "offline";
  if (s.accessibility === null) return "unknown";
  return s.accessibility ? "ok" : "needed";
}

export function microphoneState(s: NativeStatus): PermissionState {
  if (!s.connected) return "offline";
  if (s.microphone === null) return "unknown";
  return s.microphone === "granted" ? "ok" : "needed";
}

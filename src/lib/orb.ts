import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { isTauri } from "./appInfo";
import { useTauriEvent } from "./useTauriEvent";

// Mirrors src-tauri/src/activity.rs.
export type OrbState = "idle" | "listening" | "thinking" | "haunting" | "error";

export interface OrbView {
  state: OrbState;
  message: string | null;
}

export const ORB_EVENT = "orb-state";
export const IDLE: OrbView = { state: "idle", message: null };

/** BOB's voice for each state (clear first, playful second). */
export function orbLabel(v: OrbView): string {
  switch (v.state) {
    case "idle":
      return "Floating nearby";
    case "listening":
      return "Listening...";
    case "thinking":
      return "Thinking...";
    case "haunting":
      return "Haunting your caret";
    case "error":
      return v.message ? `Needs a hand: ${v.message}` : "Needs a hand";
  }
}

/** Live orb state from the core. */
export function useOrb(): OrbView {
  const [view, setView] = useState<OrbView>(IDLE);
  useEffect(() => {
    if (!isTauri()) return;
    let alive = true;
    invoke<OrbView>("orb_get")
      .then((v) => alive && setView(v))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);
  useTauriEvent<OrbView>(ORB_EVENT, setView);
  return view;
}

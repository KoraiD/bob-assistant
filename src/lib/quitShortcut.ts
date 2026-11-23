import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "./appInfo";

/** True for Cmd+Q (macOS) with no other modifier. */
export function isQuitShortcut(e: Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">): boolean {
  return e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "q";
}

/**
 * Fallback for Cmd+Q while BOB's window has focus. Normally the app menu handles it; if the web view
 * swallows the key equivalent before the menu sees it (a known class of WKWebView/wry issues), the page
 * still receives the keydown and asks the core to quit. Returns an uninstall function.
 */
export function installQuitShortcut(target: Window = window): () => void {
  const onKey = (e: KeyboardEvent) => {
    if (!isQuitShortcut(e) || !isTauri()) return;
    e.preventDefault();
    void invoke("app_quit");
  };
  target.addEventListener("keydown", onKey);
  return () => target.removeEventListener("keydown", onKey);
}

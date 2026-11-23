import { useEffect, useRef } from "react";
import { on } from "./brains";

/**
 * Subscribe to a Tauri event for the lifetime of a component.
 *
 * Tauri's `listen()` is asynchronous. A naive `listen(...).then(u => unlisten = u)` with
 * `return () => unlisten?.()` leaks a listener whenever the component unmounts before the promise
 * resolves, which React StrictMode does on every mount in development. The leaked listener then
 * delivers every event twice (seen as duplicated words in "Try it"). Here, a cleanup that runs first
 * marks the subscription cancelled, and the listener is removed as soon as it arrives.
 */
export function useTauriEvent<T>(event: string, handler: (payload: T) => void): void {
  const latest = useRef(handler);
  latest.current = handler;

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    on<T>(event, (payload) => {
      if (!cancelled) latest.current(payload);
    }).then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [event]);
}

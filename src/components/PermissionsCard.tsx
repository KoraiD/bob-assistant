import { useEffect, useState } from "react";
import {
  DISCONNECTED,
  accessibilityState,
  getNativeStatus,
  microphoneState,
  onNativeStatus,
  requestPermissions,
  type NativeStatus,
  type PermissionState,
} from "../lib/nativeStatus";

const LABEL: Record<PermissionState, string> = {
  ok: "Granted",
  needed: "Needed",
  unknown: "Checking...",
  offline: "Helper not running",
};

function Row({ name, why, state }: { name: string; why: string; state: PermissionState }) {
  const good = state === "ok";
  return (
    <li className="mac-row justify-between" data-testid={`perm-${name.toLowerCase()}`}>
      <div>
        <div className="mac-row-title">{name}</div>
        <div className="mac-row-detail">{why}</div>
      </div>
      <span
        className="flex items-center gap-1.5 text-xs"
        style={{ color: good ? "var(--accent)" : "var(--accent-2)" }}
      >
        <span aria-hidden className="inline-block h-2 w-2 rounded-full" style={{ background: "currentColor" }} />
        {LABEL[state]}
      </span>
    </li>
  );
}

/** Shows the helper's Accessibility and Microphone status and lets the user trigger the prompts. */
export function PermissionsCard() {
  const [status, setStatus] = useState<NativeStatus>(DISCONNECTED);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    getNativeStatus().then((s) => alive && setStatus(s));
    onNativeStatus((s) => alive && setStatus(s)).then((u) => {
      if (alive) unlisten = u;
      else u();
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  const ask = async () => {
    setError(null);
    try {
      await requestPermissions();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const allGood = accessibilityState(status) === "ok" && microphoneState(status) === "ok";
  const offline = !status.connected;

  return (
    <section aria-label="Permissions" className="mac-group panel-in">
      <h2 className="mac-group-title">Permissions</h2>
      <div className="mac-group-body py-1">
      <ul>
        <Row name="Accessibility" why="Read the caret position and insert text in other apps." state={accessibilityState(status)} />
        <Row name="Microphone" why="Voice dictation." state={microphoneState(status)} />
      </ul>
      {offline && (
        <p className="py-2 text-xs" style={{ color: "var(--text-muted)" }} data-testid="helper-offline">
          BOB's helper is not running yet. Open BOB from the menu bar app to check permissions.
        </p>
      )}
      {!offline && !allGood && (
        <button
          type="button"
          onClick={ask}
          className="mac-button primary my-2"
        >
          Grant permissions
        </button>
      )}
      </div>
      <p className="mac-group-footer">BOB only asks for what each feature needs. Reading the screen is opt-in (Personalize).</p>
      {error && (
        <p role="alert" className="mt-2 text-sm" style={{ color: "var(--danger)" }}>
          {error}
        </p>
      )}
    </section>
  );
}

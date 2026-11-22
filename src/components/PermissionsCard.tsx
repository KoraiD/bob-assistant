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
    <li className="flex items-center justify-between gap-4 py-2" data-testid={`perm-${name.toLowerCase()}`}>
      <div>
        <div className="font-medium">{name}</div>
        <div className="text-xs" style={{ color: "var(--text-muted)" }}>
          {why}
        </div>
      </div>
      <span
        className="rounded-full px-3 py-1 text-xs"
        style={{
          background: good ? "var(--accent-soft)" : "var(--accent-2-soft)",
          color: good ? "var(--accent)" : "var(--accent-2)",
        }}
      >
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
    <section aria-label="Permissions" className="soft-card panel-in mt-6 p-5">
      <h2 className="mb-1 text-lg font-medium">Permissions</h2>
      <p className="mb-2 text-sm" style={{ color: "var(--text-muted)" }}>
        BOB never records your screen. It only asks for what each feature needs.
      </p>
      <ul>
        <Row name="Accessibility" why="Read the caret position and insert text in other apps." state={accessibilityState(status)} />
        <Row name="Microphone" why="Voice dictation." state={microphoneState(status)} />
      </ul>
      {offline && (
        <p className="mt-3 text-sm" style={{ color: "var(--text-muted)" }} data-testid="helper-offline">
          BOB's helper is not running yet. Open BOB from the menu bar app to check permissions.
        </p>
      )}
      {!offline && !allGood && (
        <button
          type="button"
          onClick={ask}
          className="mt-3 rounded-xl px-4 py-2 text-sm font-medium"
          style={{ background: "var(--accent)", color: "var(--accent-fg)" }}
        >
          Grant permissions
        </button>
      )}
      {error && (
        <p role="alert" className="mt-2 text-sm" style={{ color: "var(--danger)" }}>
          {error}
        </p>
      )}
    </section>
  );
}

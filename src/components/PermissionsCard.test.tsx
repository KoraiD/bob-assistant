import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import {
  accessibilityState,
  microphoneState,
  DISCONNECTED,
  type NativeStatus,
} from "../lib/nativeStatus";

// Pretend we run inside Tauri and control the IPC layer.
const invoke = vi.fn();
const handlers: Array<(e: { payload: NativeStatus }) => void> = [];
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (_name: string, h: (e: { payload: NativeStatus }) => void) => {
    handlers.push(h);
    return () => {};
  },
}));

import { PermissionsCard } from "./PermissionsCard";

beforeEach(() => {
  invoke.mockReset();
  handlers.length = 0;
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
});
afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

describe("permission state mapping", () => {
  it("reports offline when the helper is not connected", () => {
    expect(accessibilityState(DISCONNECTED)).toBe("offline");
    expect(microphoneState(DISCONNECTED)).toBe("offline");
  });

  it("maps every combination without throwing", () => {
    const mics = ["granted", "denied", "undetermined", null] as const;
    for (const connected of [true, false]) {
      for (const accessibility of [true, false, null]) {
        for (const microphone of mics) {
          const s: NativeStatus = { connected, accessibility, microphone };
          expect(["ok", "needed", "unknown", "offline"]).toContain(accessibilityState(s));
          expect(["ok", "needed", "unknown", "offline"]).toContain(microphoneState(s));
        }
      }
    }
  });

  it("treats only 'granted' as a good microphone state", () => {
    const base = { connected: true, accessibility: true };
    expect(microphoneState({ ...base, microphone: "granted" })).toBe("ok");
    expect(microphoneState({ ...base, microphone: "denied" })).toBe("needed");
    expect(microphoneState({ ...base, microphone: "undetermined" })).toBe("needed");
  });
});

describe("PermissionsCard", () => {
  it("shows the status from the core and updates on events", async () => {
    invoke.mockResolvedValueOnce({ connected: true, accessibility: false, microphone: "undetermined" });
    render(<PermissionsCard />);
    await waitFor(() => expect(screen.getByTestId("perm-accessibility")).toHaveTextContent("Needed"));

    handlers.forEach((h) => h({ payload: { connected: true, accessibility: true, microphone: "granted" } }));
    await waitFor(() => expect(screen.getByTestId("perm-accessibility")).toHaveTextContent("Granted"));
    expect(screen.getByTestId("perm-microphone")).toHaveTextContent("Granted");
    expect(screen.queryByRole("button", { name: /grant permissions/i })).not.toBeInTheDocument();
  });

  it("asks the core to show the prompts when the button is pressed", async () => {
    invoke.mockImplementation(async (cmd: string) =>
      cmd === "native_status" ? { connected: true, accessibility: false, microphone: "denied" } : undefined,
    );
    render(<PermissionsCard />);
    await userEvent.click(await screen.findByRole("button", { name: /grant permissions/i }));
    expect(invoke).toHaveBeenCalledWith("request_permissions");
  });

  it("explains the offline helper instead of offering a button that cannot work", async () => {
    invoke.mockResolvedValue(DISCONNECTED);
    render(<PermissionsCard />);
    expect(await screen.findByTestId("helper-offline")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /grant permissions/i })).not.toBeInTheDocument();
  });

  it("shows a readable error when the request fails", async () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "native_status") return { connected: true, accessibility: false, microphone: "denied" };
      throw new Error("native helper is not connected");
    });
    render(<PermissionsCard />);
    await userEvent.click(await screen.findByRole("button", { name: /grant permissions/i }));
    expect(await screen.findByRole("alert")).toHaveTextContent("not connected");
  });
});

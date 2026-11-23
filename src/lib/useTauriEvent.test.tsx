import { act, render } from "@testing-library/react";
import { StrictMode } from "react";
import { vi } from "vitest";

// A fake event bus with an asynchronous listen(), like Tauri's.
type Handler = (e: { payload: unknown }) => void;
const listeners = new Set<Handler>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (_name: string, h: Handler) => {
    await Promise.resolve();
    listeners.add(h);
    return () => listeners.delete(h);
  },
}));

import { useTauriEvent } from "./useTauriEvent";

function emit(payload: unknown) {
  listeners.forEach((h) => h({ payload }));
}

function Probe({ onToken }: { onToken: (t: string) => void }) {
  useTauriEvent<string>("brain-token", onToken);
  return null;
}

beforeEach(() => {
  listeners.clear();
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
});
afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

describe("useTauriEvent", () => {
  it("delivers each event exactly once under StrictMode double mounting", async () => {
    const got: string[] = [];
    render(
      <StrictMode>
        <Probe onToken={(t) => got.push(t)} />
      </StrictMode>,
    );
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(listeners.size).toBe(1);
    emit("ow");
    emit(" what");
    expect(got).toEqual(["ow", " what"]);
  });

  it("removes the listener on unmount, even if unmounted before listen() resolved", async () => {
    const { unmount } = render(<Probe onToken={() => {}} />);
    unmount();
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(listeners.size).toBe(0);
  });
});

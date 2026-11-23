import { vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import { installQuitShortcut, isQuitShortcut } from "./quitShortcut";

const key = (k: string, mods: Partial<KeyboardEventInit> = {}) => new KeyboardEvent("keydown", { key: k, ...mods, cancelable: true });

describe("Cmd+Q fallback", () => {
  beforeEach(() => {
    invoke.mockReset();
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
  });
  afterEach(() => {
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  });

  it("recognizes only plain Cmd+Q", () => {
    expect(isQuitShortcut(key("q", { metaKey: true }))).toBe(true);
    expect(isQuitShortcut(key("Q", { metaKey: true }))).toBe(true);
    for (const e of [key("q"), key("q", { ctrlKey: true }), key("q", { metaKey: true, shiftKey: true }), key("q", { metaKey: true, altKey: true }), key("w", { metaKey: true })]) {
      expect(isQuitShortcut(e)).toBe(false);
    }
  });

  it("asks the core to quit and can be uninstalled", () => {
    const off = installQuitShortcut();
    const e = key("q", { metaKey: true });
    window.dispatchEvent(e);
    expect(invoke).toHaveBeenCalledWith("app_quit");
    expect(e.defaultPrevented).toBe(true);
    off();
    window.dispatchEvent(key("q", { metaKey: true }));
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("does nothing outside the app", () => {
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
    const off = installQuitShortcut();
    window.dispatchEvent(key("q", { metaKey: true }));
    expect(invoke).not.toHaveBeenCalled();
    off();
  });
});

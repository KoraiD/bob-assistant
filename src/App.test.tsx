import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
const listeners: Record<string, (e: { payload: unknown }) => void> = {};
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (name: string, cb: (e: { payload: unknown }) => void) => {
    listeners[name] = cb;
    return () => delete listeners[name];
  },
}));

import App, { CAPABILITIES, formatBuild, PANELS } from "./App";
import { ONBOARDED_KEY, STEPS } from "./components/OnboardingWizard";
import { BROWSER_APP_INFO, getAppInfo, isTauri } from "./lib/appInfo";
import { orbLabel } from "./lib/orb";
import { isDaylight, loadTheme, THEME_KEY } from "./lib/theme";

afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  localStorage.clear();
  document.documentElement.classList.remove("daylight");
  invoke.mockReset();
});

/** What the core answers for the commands every panel calls on mount (the real core always answers). */
const DEFAULTS: Record<string, unknown> = {
  app_info: { name: "BOB", version: "0.1.0", platform: "macos", arch: "aarch64" },
  native_status: { connected: true, accessibility: false, microphone: "undetermined" },
  orb_get: { state: "idle", message: null },
  models_list: [],
  dictation_get: { settings: {}, models: [], stats: {}, microphone: null },
};
function mockCore(overrides: Record<string, unknown> = {}) {
  invoke.mockImplementation(async (cmd: string) => ({ ...DEFAULTS, ...overrides })[cmd]);
}

/** Accessibility audit with axe-core. Color contrast is checked from tokens instead (jsdom has no layout). */
async function audit(container: HTMLElement) {
  const r = await axe.run(container, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
  return r.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
}

describe("App shell", () => {
  it("renders BOB and all three capabilities in priority order", () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: "BOB" })).toBeInTheDocument();
    expect(CAPABILITIES.map((c) => c.id)).toEqual(["ghost", "selection", "dictation"]);
    for (const c of CAPABILITIES) {
      expect(screen.getByTestId(`capability-${c.id}`)).toBeInTheDocument();
    }
  });

  it("exposes an accessible name for the ghost mark", () => {
    render(<App />);
    expect(screen.getAllByRole("img", { name: /friendly ghost/i }).length).toBeGreaterThan(0);
  });

  it("has a sidebar with every panel (UI-1) and switches panels by keyboard", async () => {
    render(<App />);
    const nav = screen.getByRole("navigation", { name: "Sections" });
    const buttons = within(nav).getAllByRole("button");
    expect(buttons).toHaveLength(PANELS.length);
    buttons.forEach((b, i) => expect(b.textContent?.endsWith(PANELS[i].label)).toBe(true));
    expect(within(nav).getByRole("button", { name: /Overview/ })).toHaveAttribute("aria-current", "page");
    within(nav).getByRole("button", { name: /About/ }).focus();
    await userEvent.keyboard("{Enter}");
    expect(screen.getByRole("main", { name: "About" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Appearance" })).toBeInTheDocument();
  });

  it("Open on a capability goes to its panel", async () => {
    render(<App />);
    await userEvent.click(screen.getByRole("button", { name: "Open Voice Dictation" }));
    expect(screen.getByRole("main", { name: "Voice Dictation" })).toBeInTheDocument();
  });

  it.each(PANELS.map((p) => p.id))("panel %s has no axe violations (UI-8)", async (id) => {
    const { container } = render(<App />);
    await userEvent.click(within(screen.getByRole("navigation")).getByRole("button", { name: new RegExp(PANELS.find((p) => p.id === id)!.label) }));
    expect(await audit(container)).toEqual([]);
  });
});

describe("theme (UI-2)", () => {
  it("defaults to dark and switches to daylight from About", async () => {
    render(<App />);
    expect(document.documentElement.classList.contains("daylight")).toBe(false);
    await userEvent.click(screen.getByRole("button", { name: /About/ }));
    await userEvent.click(screen.getByRole("radio", { name: "Daylight" }));
    expect(document.documentElement.classList.contains("daylight")).toBe(true);
    expect(localStorage.getItem(THEME_KEY)).toBe("daylight");
  });

  it("system follows the OS preference; unknown values fall back to dark", () => {
    expect(isDaylight("system", true)).toBe(true);
    expect(isDaylight("system", false)).toBe(false);
    expect(isDaylight("dark", true)).toBe(false);
    expect(loadTheme({ getItem: () => "neon" })).toBe("dark");
  });
});

describe("orb (UI-3)", () => {
  it("follows orb-state events from the core", async () => {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    localStorage.setItem(ONBOARDED_KEY, "1");
    mockCore();
    render(<App />);
    const orb = screen.getByTestId("orb");
    await vi.waitFor(() => expect(listeners["orb-state"]).toBeDefined());
    act(() => listeners["orb-state"]({ payload: { state: "listening", message: null } }));
    expect(orb).toHaveAttribute("data-state", "listening");
    expect(within(orb).getByRole("status")).toHaveTextContent("Listening...");
    act(() => listeners["orb-state"]({ payload: { state: "error", message: "I didn't hear anything." } }));
    expect(within(orb).getByRole("status")).toHaveTextContent("Needs a hand: I didn't hear anything.");
  });

  it("has a label for every state", () => {
    for (const state of ["idle", "listening", "thinking", "haunting", "error"] as const) {
      expect(orbLabel({ state, message: null }).length).toBeGreaterThan(3);
    }
  });
});

describe("first-run wizard (UI-4)", () => {
  it("opens on first run in the app, walks every step and is remembered", async () => {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    mockCore({
      models_list: [{ id: "qwen", name: "Qwen2.5 1.5B", role: "", file: "a.gguf", size: 1_117_320_736, license: "Apache-2.0", min_ram_gb: 16, installed: false, active: false }],
      dictation_get: { settings: {}, models: [{ id: "whisper", name: "Whisper turbo", role: "", file: "w.bin", size: 574_041_195, license: "MIT", min_ram_gb: 16, installed: true, active: false }], stats: {}, microphone: null },
    });
    const { container, unmount } = render(<App />);
    const dialog = await screen.findByRole("dialog", { name: "Boo! I'm BOB." });
    expect(await audit(container)).toEqual([]);
    await userEvent.click(within(dialog).getByRole("button", { name: "Next" }));
    expect(await within(dialog).findByText("Accessibility: not yet")).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Ask macOS for access" }));
    expect(invoke).toHaveBeenCalledWith("request_permissions");
    await userEvent.click(within(dialog).getByRole("button", { name: "Next" }));
    await userEvent.click(await within(dialog).findByRole("button", { name: "Download" }));
    expect(invoke).toHaveBeenCalledWith("model_download", { id: "qwen" });
    await userEvent.click(within(dialog).getByRole("button", { name: "Next" }));
    expect(await within(dialog).findByText("✅ Installed")).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Next" }));
    await userEvent.click(within(dialog).getByRole("button", { name: "Start haunting" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(localStorage.getItem(ONBOARDED_KEY)).toBe("1");
    unmount();
    render(<App />);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(STEPS).toHaveLength(5);
  });

  it("Escape skips it, and About reopens it", async () => {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    mockCore();
    render(<App />);
    const dialog = await screen.findByRole("dialog");
    fireEvent.keyDown(dialog, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: /About/ }));
    await userEvent.click(screen.getByRole("button", { name: "Show the welcome tour" }));
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("never opens in the browser preview", () => {
    render(<App />);
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

describe("appInfo", () => {
  it("falls back to browser info outside Tauri", async () => {
    expect(isTauri()).toBe(false);
    await expect(getAppInfo()).resolves.toEqual(BROWSER_APP_INFO);
  });
});

describe("formatBuild", () => {
  it("labels the browser preview plainly", () => {
    expect(formatBuild(BROWSER_APP_INFO)).toBe("Browser preview");
  });
  it("shows version, platform and arch inside the app", () => {
    expect(formatBuild({ name: "BOB", version: "0.1.0", platform: "macos", arch: "aarch64" })).toBe(
      "BOB 0.1.0 · macos aarch64",
    );
  });
});

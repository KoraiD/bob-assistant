import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { hotkeyPreset, validCustom, type SelectionView } from "../lib/selection";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import { SelectionCard } from "./SelectionCard";

let view: SelectionView;

beforeEach(() => {
  view = {
    settings: { hotkey: { key_code: 38, modifiers: ["cmd", "shift"] }, custom: [] },
    actions: ["Fix grammar", "Rephrase", "Summarize", "Translate to English"],
    stats: { menus: 2, runs: 2, replaced: 1, failed: 1, last_error: "Nothing changed: the selection changed.", last_ms: 1500 },
  };
  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "selection_get") return view;
    if (cmd === "selection_set_settings") {
      view = { ...view, settings: args!.settings as SelectionView["settings"] };
      return undefined;
    }
    return undefined;
  });
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
});
afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

describe("SelectionCard", () => {
  it("shows the hotkey, the actions, usage and the last problem", async () => {
    render(<SelectionCard />);
    expect(await screen.findByTestId("selection-summary")).toHaveTextContent("⌘⇧J");
    expect(screen.getByTestId("selection-actions")).toHaveTextContent("Fix grammar · Rephrase");
    expect(screen.getByTestId("selection-stats")).toHaveTextContent("2 run · 1 replaced · 1 failed · last took 1.5 s");
    expect(screen.getByRole("alert")).toHaveTextContent("the selection changed");
  });

  it("changes the hotkey", async () => {
    render(<SelectionCard />);
    await userEvent.selectOptions(await screen.findByLabelText("Selection Actions hotkey"), "ctrl-opt-space");
    expect(invoke).toHaveBeenCalledWith("selection_set_settings", {
      settings: { hotkey: { key_code: 49, modifiers: ["control", "option"] }, custom: [] },
    });
  });

  it("adds and saves a custom action, dropping empty rows", async () => {
    render(<SelectionCard />);
    const box = await screen.findByTestId("custom-actions");
    await userEvent.click(within(box).getByText(/Your own actions/));
    await userEvent.click(within(box).getByRole("button", { name: "Add action" }));
    await userEvent.click(within(box).getByRole("button", { name: "Add action" }));
    await userEvent.type(within(box).getByLabelText("Action 1 name"), "Decline politely");
    await userEvent.type(within(box).getByLabelText("Action 1 instruction"), "Turn this into a polite no.");
    await userEvent.click(within(box).getByRole("button", { name: "Save actions" }));
    expect(invoke).toHaveBeenCalledWith("selection_set_settings", {
      settings: expect.objectContaining({ custom: [{ label: "Decline politely", instruction: "Turn this into a polite no." }] }),
    });
  });
});

describe("selection helpers", () => {
  it("matches presets regardless of modifier order", () => {
    expect(hotkeyPreset({ key_code: 38, modifiers: ["shift", "cmd"] })?.id).toBe("cmd-shift-j");
    expect(hotkeyPreset({ key_code: 1, modifiers: ["cmd"] })).toBeUndefined();
  });

  it("keeps only complete custom actions, trimmed and bounded", () => {
    const many = Array.from({ length: 20 }, (_, i) => ({ label: ` A  ${i} `, instruction: " do it " }));
    const out = validCustom([{ label: "", instruction: "x" }, ...many]);
    expect(out).toHaveLength(12);
    expect(out[0]).toEqual({ label: "A 0", instruction: "do it" });
  });
});

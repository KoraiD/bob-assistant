import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import type { ContextView } from "../lib/context";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import { PersonalizePanel } from "./PersonalizePanel";

let view: ContextView;

beforeEach(() => {
  view = {
    settings: { scope: "accessibility", tone: "natural", language: "", ghost: { profile: false, memory: false, window: false, screen: false } },
    profile: "",
    profile_path: "/Users/me/Library/Application Support/dev.bob.assistant/profile.md",
    template: "# About me ✍️\n",
    max_profile_bytes: 32768,
    visible: { app_name: "Mail", has_window_title: true, after_caret_chars: 0, screen_chars: 0, screen_ms: null, screen_age_s: null },
    screen_recording: false,
  };
  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "context_get":
        return view;
      case "context_set_settings":
        view = { ...view, settings: args!.settings as ContextView["settings"] };
        return undefined;
      case "profile_save":
        view = { ...view, profile: args!.text as string };
        return undefined;
      case "context_preview":
        return { ghost_notes: "", chat_system: "You are BOB" };
      default:
        return undefined;
    }
  });
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
});
afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

describe("PersonalizePanel", () => {
  it("edits, formats and saves the profile, and previews Markdown with emoji", async () => {
    render(<PersonalizePanel />);
    await userEvent.click(await screen.findByRole("button", { name: "Start from a template" }));
    const area = screen.getByLabelText("Profile (Markdown)") as HTMLTextAreaElement;
    expect(area.value).toBe("# About me ✍️\n");
    await userEvent.type(area, "I love :jack_o_lantern: season");
    await userEvent.click(screen.getByRole("button", { name: "Bold" }));
    expect(area.value).toContain("**bold text**");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(invoke).toHaveBeenCalledWith("profile_save", { text: area.value });
    expect(await screen.findByRole("status")).toHaveTextContent("Saved");
    await userEvent.click(screen.getByRole("tab", { name: "Preview" }));
    const preview = screen.getByTestId("profile-preview");
    expect(preview.querySelector("h1")).toHaveTextContent("About me ✍️");
    expect(preview).toHaveTextContent("I love 🎃 season");
    expect(preview.querySelector("strong")).toHaveTextContent("bold text");
  });

  it("never renders live links or remote images in the preview", async () => {
    view.profile = "[site](https://example.com) ![me](https://example.com/me.png)";
    render(<PersonalizePanel />);
    await userEvent.click(await screen.findByRole("tab", { name: "Preview" }));
    const preview = screen.getByTestId("profile-preview");
    expect(preview.querySelector("a")).toBeNull();
    expect(preview.querySelector("img")).toBeNull();
    expect(preview).toHaveTextContent("[image: me]");
  });

  it("switches the visible-context scope and explains Screen Recording", async () => {
    render(<PersonalizePanel />);
    await userEvent.click(await screen.findByRole("radio", { name: /Active window/ }));
    expect(invoke).toHaveBeenCalledWith("context_set_settings", {
      settings: expect.objectContaining({ scope: "active_window" }),
    });
    expect(await screen.findByTestId("screen-permission")).toHaveTextContent("Screen Recording is off");
  });

  it("toggles Ghost Text layers and shows the preview on request", async () => {
    render(<PersonalizePanel />);
    const windowBox = await screen.findByRole("checkbox", { name: /Also use the window title/ });
    expect(windowBox).toBeDisabled();
    await userEvent.click(screen.getByRole("checkbox", { name: "Use my remembered phrases" }));
    expect(invoke).toHaveBeenCalledWith("context_set_settings", {
      settings: expect.objectContaining({ ghost: { profile: false, memory: true, window: false, screen: false } }),
    });
    await waitFor(() => expect(screen.getByRole("checkbox", { name: /Also use the window title/ })).toBeEnabled());
    await userEvent.click(screen.getByRole("checkbox", { name: /Also use the window title/ }));
    expect(invoke).toHaveBeenCalledWith("context_set_settings", {
      settings: expect.objectContaining({ ghost: { profile: false, memory: true, window: true, screen: false } }),
    });
    expect(invoke).not.toHaveBeenCalledWith("context_preview");
    await userEvent.click(screen.getByRole("button", { name: "Show preview" }));
    await waitFor(() => expect(screen.getByTestId("preview-chat")).toHaveTextContent("You are BOB"));
    expect(screen.getByTestId("preview-ghost")).toHaveTextContent("every Ghost Text layer is off");
  });
});

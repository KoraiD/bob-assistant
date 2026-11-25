import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import type { GhostMemory, GhostStatus, Snapshot } from "../lib/brains";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));

import { GhostCard } from "./GhostCard";

const snapshot: Snapshot = {
  brains: [],
  roles: { ghost: "builtin", selection: null, dictation: null },
  builtin: { state: "ready", base_url: "http://127.0.0.1:5000" },
  builtin_model: null,
};
let status: GhostStatus;
let memory: GhostMemory;

beforeEach(() => {
  status = {
    enabled: true,
    quiet_apps: [{ bundle_id: "com.googlecode.iterm2", name: "iTerm" }],
    chip_offsets: [],
    recent_apps: [{ bundle_id: "com.apple.Notes", name: "Notes" }],
    typo_fixes: true,
    mid_sentence: true,
    memory_count: 2,
    p50_ms: null,
    p95_ms: null,
    stats: { requests: 0, chips_shown: 0, accepted_words: 0, accepted_all: 0, dismissed: 0, errors: 0, last_error: null, typing_through: 0 },
  };
  memory = { phrases: [{ text: "as soon as I can", uses: 3 }, { text: "lazy dog", uses: 1 }] };
  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "ghost_status":
        return status;
      case "brain_snapshot":
        return snapshot;
      case "ghost_memory":
        return memory;
      case "ghost_memory_forget":
        memory = { phrases: memory.phrases.filter((p) => p.text !== args!.text) };
        status = { ...status, memory_count: memory.phrases.length };
        return true;
      case "running_apps":
        return [{ bundle_id: "com.apple.Safari", name: "Safari" }, { bundle_id: "com.googlecode.iterm2", name: "iTerm" }];
      case "ghost_memory_add": {
        const text = String(args!.text).trim();
        if (text.length < 2) return false;
        memory = { phrases: [{ text, uses: 1 }, ...memory.phrases] };
        status = { ...status, memory_count: memory.phrases.length };
        return true;
      }
      case "ghost_memory_edit":
        memory = { phrases: memory.phrases.map((p) => (p.text === args!.old ? { ...p, text: String(args!.new) } : p)) };
        return true;
      case "ghost_memory_clear":
        memory = { phrases: [] };
        status = { ...status, memory_count: 0 };
        return undefined;
      case "ghost_set_quiet": {
        const app = { bundle_id: args!.bundleId as string, name: args!.name as string };
        status = {
          ...status,
          quiet_apps: args!.quiet ? [...status.quiet_apps, app] : status.quiet_apps.filter((a) => a.bundle_id !== app.bundle_id),
        };
        return undefined;
      }
      case "ghost_set_chip_offset": {
        const row = { bundle_id: args!.bundleId as string, name: args!.name as string, dx: args!.dx as number, dy: args!.dy as number };
        const rest = status.chip_offsets.filter((o) => o.bundle_id !== row.bundle_id);
        status = { ...status, chip_offsets: row.dx === 0 && row.dy === 0 ? rest : [...rest, row] };
        return undefined;
      }
      default:
        return undefined;
    }
  });
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
});
afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

describe("GhostCard", () => {
  it("removes a quiet app, adds one from running and recent apps, or by bundle id", async () => {
    render(<GhostCard />);
    await userEvent.click(await screen.findByText("Quiet apps"));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("running_apps"));
    expect(screen.getByRole("list", { name: "Quiet apps" })).toHaveTextContent("iTerm");
    // Running and recent apps that are not quiet yet are offered.
    const add = await screen.findByRole("button", { name: "Quiet in Safari" });
    expect(screen.getByRole("button", { name: "Quiet in Notes" })).toBeInTheDocument();
    await userEvent.click(add);
    expect(invoke).toHaveBeenCalledWith("ghost_set_quiet", { bundleId: "com.apple.Safari", name: "Safari", quiet: true });
    await waitFor(() => expect(screen.getByRole("list", { name: "Quiet apps" })).toHaveTextContent("Safari"));
    // Search narrows both lists.
    await userEvent.type(screen.getByLabelText("Search apps"), "term");
    expect(screen.getByRole("list", { name: "Quiet apps" })).not.toHaveTextContent("Safari");
    expect(screen.queryByRole("button", { name: "Quiet in Notes" })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Let Ghost Text haunt iTerm again" }));
    expect(invoke).toHaveBeenCalledWith("ghost_set_quiet", { bundleId: "com.googlecode.iterm2", name: "iTerm", quiet: false });
    // A typed bundle id is checked before it is sent.
    await userEvent.type(screen.getByLabelText("Bundle id"), "not a bundle{Enter}");
    expect(await screen.findByRole("alert")).toHaveTextContent("com.company.App");
    await userEvent.clear(screen.getByLabelText("Bundle id"));
    await userEvent.type(screen.getByLabelText("Bundle id"), "com.example.Secret{Enter}");
    expect(invoke).toHaveBeenCalledWith("ghost_set_quiet", { bundleId: "com.example.Secret", name: "", quiet: true });
  });

  it("fetches memory only when opened; adds, edits, forgets and clears with confirmation", async () => {
    render(<GhostCard />);
    const summary = await screen.findByText("Remembered phrases");
    expect(invoke).not.toHaveBeenCalledWith("ghost_memory");
    await userEvent.click(summary);
    expect(await screen.findByText("as soon as I can")).toBeInTheDocument();
    expect(screen.getByText("×3")).toBeInTheDocument();

    await userEvent.type(screen.getByLabelText("New phrase"), "Best regards{Enter}");
    expect(invoke).toHaveBeenCalledWith("ghost_memory_add", { text: "Best regards" });
    expect(await screen.findByText("Best regards")).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("New phrase"), "x{Enter}");
    expect(await screen.findByRole("alert")).toHaveTextContent("at least two letters");

    await userEvent.click(screen.getByRole("button", { name: 'Edit "lazy dog"' }));
    const input = screen.getByRole("textbox", { name: 'Edit "lazy dog"' });
    await userEvent.clear(input);
    await userEvent.type(input, "lazy fox{Enter}");
    expect(invoke).toHaveBeenCalledWith("ghost_memory_edit", { old: "lazy dog", new: "lazy fox" });
    expect(await screen.findByText("lazy fox")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: 'Forget "lazy fox"' }));
    expect(invoke).toHaveBeenCalledWith("ghost_memory_forget", { text: "lazy fox" });
    await waitFor(() => expect(screen.queryByText("lazy fox")).not.toBeInTheDocument());

    await userEvent.click(screen.getByRole("button", { name: "Forget all…" }));
    expect(invoke).not.toHaveBeenCalledWith("ghost_memory_clear");
    await userEvent.click(within(screen.getByRole("group", { name: "Confirm forgetting" })).getByRole("button", { name: "Forget all" }));
    expect(invoke).toHaveBeenCalledWith("ghost_memory_clear");
    expect(await screen.findByText("Nothing remembered yet.")).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByRole("button", { name: "Forget all…" })).not.toBeInTheDocument());
  });

  it("turns typo fixes and mid-sentence suggestions on and off", async () => {
    render(<GhostCard />);
    await userEvent.click(await screen.findByRole("switch", { name: "Fix typos" }));
    expect(invoke).toHaveBeenCalledWith("ghost_set_option", { option: "typo_fixes", on: false });
    await userEvent.click(screen.getByRole("switch", { name: "Suggest inside a sentence" }));
    expect(invoke).toHaveBeenCalledWith("ghost_set_option", { option: "mid_sentence", on: false });
  });
  it("nudges the chip in one app and resets it", async () => {
    render(<GhostCard />);
    const box = await screen.findByTestId("chip-offsets");
    await userEvent.click(within(box).getByText(/Chip position per app/));
    expect(within(box).getByText(/No nudges/)).toBeInTheDocument();
    await userEvent.selectOptions(within(box).getByLabelText("Adjust the chip in another app"), "com.apple.Notes");
    await userEvent.click(within(box).getByRole("button", { name: "Move the chip down in Notes" }));
    expect(invoke).toHaveBeenCalledWith("ghost_set_chip_offset", { bundleId: "com.apple.Notes", name: "Notes", dx: 0, dy: 1 });
    await waitFor(() => expect(within(box).getByTestId("offset-com.apple.Notes")).toHaveTextContent("+0, +1 pt"));
    await userEvent.click(within(box).getByRole("button", { name: "Move the chip left in Notes" }));
    await waitFor(() => expect(within(box).getByTestId("offset-com.apple.Notes")).toHaveTextContent("-1, +1 pt"));
    await userEvent.click(within(box).getByRole("button", { name: "Reset the chip position in Notes" }));
    expect(invoke).toHaveBeenCalledWith("ghost_set_chip_offset", { bundleId: "com.apple.Notes", name: "Notes", dx: 0, dy: 0 });
  });
});

import { render, screen, waitFor } from "@testing-library/react";
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
    recent_apps: [{ bundle_id: "com.apple.Notes", name: "Notes" }],
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
  it("lists quiet and recent apps and silences an app", async () => {
    render(<GhostCard />);
    const notes = await screen.findByRole("switch", { name: "Quiet in Notes" });
    expect(notes).toHaveAttribute("aria-checked", "false");
    expect(screen.getByRole("switch", { name: "Quiet in iTerm" })).toHaveAttribute("aria-checked", "true");
    await userEvent.click(notes);
    expect(invoke).toHaveBeenCalledWith("ghost_set_quiet", { bundleId: "com.apple.Notes", name: "Notes", quiet: true });
    await waitFor(() => expect(screen.getByRole("switch", { name: "Quiet in Notes" })).toHaveAttribute("aria-checked", "true"));
  });

  it("fetches memory only when opened, and forgets it on request", async () => {
    render(<GhostCard />);
    const summary = await screen.findByText("Remembered phrases");
    expect(invoke).not.toHaveBeenCalledWith("ghost_memory");
    await userEvent.click(summary);
    expect(await screen.findByText("as soon as I can")).toBeInTheDocument();
    expect(screen.getByText("×3")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: 'Forget "lazy dog"' }));
    expect(invoke).toHaveBeenCalledWith("ghost_memory_forget", { text: "lazy dog" });
    await waitFor(() => expect(screen.queryByText("lazy dog")).not.toBeInTheDocument());
    await userEvent.click(screen.getByRole("button", { name: "Forget all" }));
    expect(invoke).toHaveBeenCalledWith("ghost_memory_clear");
    expect(await screen.findByText("Nothing remembered yet.")).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByRole("button", { name: "Forget all" })).not.toBeInTheDocument());
  });
});

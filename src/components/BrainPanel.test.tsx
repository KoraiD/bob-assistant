import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import type { ModelInfo, Snapshot } from "../lib/brains";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));

import { BrainPanel } from "./BrainPanel";

const snapshot: Snapshot = {
  brains: [
    { id: "builtin", name: "BOB built-in (local)", kind: "builtin", endpoint: "http://127.0.0.1:0", model: null, capabilities: { ghost: true, chat: true }, has_api_key: false, is_cloud: false, shares_context: true },
    { id: "cloud", name: "Cloud", kind: "open_ai_compatible", endpoint: "https://api.example.com", model: "gpt-x", capabilities: { ghost: false, chat: true }, has_api_key: true, is_cloud: true, shares_context: false },
  ],
  roles: { ghost: "builtin", selection: "cloud", dictation: null },
  builtin: { state: "ready", base_url: "http://127.0.0.1:5000" },
  builtin_model: "qwen2.5-1.5b-instruct-q4km",
};
const models: ModelInfo[] = [
  { id: "qwen2.5-1.5b-instruct-q4km", name: "Qwen2.5 1.5B Instruct (Q4_K_M)", role: "Default", file: "a.gguf", size: 1_117_320_736, license: "Apache-2.0", min_ram_gb: 16, installed: true, active: true },
  { id: "qwen3-4b", name: "Qwen3 4B", role: "Quality", file: "b.gguf", size: 2_497_281_120, license: "Apache-2.0", min_ram_gb: 16, installed: false, active: false },
];

beforeEach(() => {
  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string) => (cmd === "brain_snapshot" ? snapshot : cmd === "models_list" ? models : undefined));
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
});
afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

describe("BrainPanel", () => {
  it("edits a brain and keeps the stored key unless replaced", async () => {
    render(<BrainPanel />);
    await userEvent.click(await screen.findByRole("button", { name: "Edit Cloud" }));
    const form = screen.getByRole("form", { name: "Edit Cloud" });
    const name = within(form).getByLabelText("Name");
    await userEvent.clear(name);
    await userEvent.type(name, "Work cloud");
    await userEvent.click(within(form).getByRole("button", { name: "Save changes" }));
    expect(invoke).toHaveBeenCalledWith("brain_upsert", {
      config: expect.objectContaining({ id: "cloud", name: "Work cloud", endpoint: "https://api.example.com" }),
      apiKey: null,
    });
  });

  it("asks before removing a brain", async () => {
    render(<BrainPanel />);
    await userEvent.click(await screen.findByRole("button", { name: "Remove Cloud" }));
    expect(invoke).not.toHaveBeenCalledWith("brain_remove", expect.anything());
    await userEvent.click(screen.getByRole("button", { name: "Confirm removing Cloud" }));
    expect(invoke).toHaveBeenCalledWith("brain_remove", { id: "cloud" });
  });

  it("shows the personal-context switch per brain, off for the cloud brain", async () => {
    render(<BrainPanel />);
    const cloud = await screen.findByRole("checkbox", { name: "Share personal context with Cloud" });
    expect(cloud).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Share personal context with BOB built-in (local)" })).toBeChecked();
    await userEvent.click(cloud);
    expect(invoke).toHaveBeenCalledWith("brain_set_share_context", { id: "cloud", share: true });
  });

  it("labels cloud brains and shows the built-in state and catalog", async () => {
    render(<BrainPanel />);
    const cloud = await screen.findByTestId("brain-cloud");
    expect(cloud).toHaveTextContent("cloud: text leaves your Mac");
    expect(cloud).toHaveTextContent("key in Keychain");
    expect(screen.getByTestId("brain-builtin")).toHaveTextContent("on this Mac");
    expect(screen.getByText("Awake")).toBeInTheDocument();
    expect(within(screen.getByTestId("model-qwen3-4b")).getByRole("button", { name: "Download" })).toBeInTheDocument();
    expect(screen.getByTestId("model-qwen2.5-1.5b-instruct-q4km")).toHaveTextContent("in use");
  });

  it("only offers ghost-capable brains for Ghost Text", async () => {
    render(<BrainPanel />);
    const ghost = await screen.findByLabelText("Ghost Text brain");
    const options = within(ghost).getAllByRole("option").map((o) => o.textContent);
    expect(options).toEqual(["Off", "BOB built-in (local)"]);
    const selection = screen.getByLabelText("Selection Actions brain");
    expect(within(selection).getAllByRole("option").map((o) => o.textContent)).toContain("Cloud (cloud)");
  });

  it("validates the add form and sends the key separately from the config", async () => {
    render(<BrainPanel />);
    await screen.findByTestId("brain-cloud");
    const form = screen.getByRole("form", { name: "Add a brain" });
    await userEvent.click(within(form).getByRole("button", { name: "Add brain" }));
    expect(within(form).getByRole("alert")).toHaveTextContent("Give the brain a name.");

    await userEvent.type(within(form).getByPlaceholderText("My Ollama"), "Home Ollama");
    await userEvent.click(within(form).getByRole("button", { name: "Add brain" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("brain_upsert", expect.objectContaining({ apiKey: null })));
    const call = invoke.mock.calls.find((c) => c[0] === "brain_upsert")!;
    expect(call[1].config).toMatchObject({ id: "home-ollama", kind: "ollama", endpoint: "http://127.0.0.1:11434" });
  });

  it("shows a friendly message in the browser preview", () => {
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
    render(<BrainPanel />);
    expect(screen.getByTestId("brains-preview")).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalled();
  });
});

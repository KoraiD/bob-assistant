import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { dictationSummary, type DictationView } from "../lib/dictation";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));

import { DictationCard } from "./DictationCard";

const model = (id: string, installed: boolean) => ({
  id,
  name: id === "whisper-base" ? "Whisper base" : "Whisper large-v3 turbo (Q5_0)",
  role: "test",
  file: `${id}.bin`,
  size: 574_041_195,
  license: "MIT",
  min_ram_gb: 16,
  installed,
  active: false,
});

let view: DictationView;

beforeEach(() => {
  view = {
    settings: { enabled: true, model: "whisper-large-v3-turbo-q5", language: "", hotkey: { key_code: 49, modifiers: ["option"] }, cleanup: "none", tap_to_toggle: false, vocabulary: ["Kiroween"], replacements: [], unload_after_min: 0 },
    models: [model("whisper-large-v3-turbo-q5", true), model("whisper-base", false)],
    stats: { dictations: 3, discarded: 1, errors: 0, last_error: null, last_audio_ms: 3200, last_transcribe_ms: 260, last_release_to_insert_ms: 290, model_loaded: true, model_error: null, idle_unloaded: false, last_available: false },
    microphone: "granted",
  };
  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "dictation_get") return view;
    if (cmd === "dictation_set_settings") {
      view = { ...view, settings: args!.settings as DictationView["settings"] };
      return undefined;
    }
    return undefined;
  });
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
});
afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

describe("DictationCard", () => {
  it("shows readiness, the hotkey, models and timings", async () => {
    render(<DictationCard />);
    expect(await screen.findByTestId("dictation-summary")).toHaveTextContent("Hold ⌥Space, speak, release.");
    expect(screen.getByTestId("speech-whisper-large-v3-turbo-q5")).toHaveTextContent("loaded");
    expect(screen.getByTestId("dictation-stats")).toHaveTextContent("3 dictated · last: 3.2 s of speech, transcribed in 260 ms, typed 290 ms after release");
  });

  it("downloads a model and changes language, cleanup and the hotkey", async () => {
    render(<DictationCard />);
    await userEvent.click(await screen.findByRole("button", { name: "Download" }));
    expect(invoke).toHaveBeenCalledWith("model_download", { id: "whisper-base" });
    await userEvent.selectOptions(screen.getByLabelText("Dictation language"), "hu");
    expect(invoke).toHaveBeenCalledWith("dictation_set_settings", { settings: expect.objectContaining({ language: "hu" }) });
    await userEvent.selectOptions(screen.getByLabelText("Dictation cleanup"), "clean");
    expect(invoke).toHaveBeenCalledWith("dictation_set_settings", { settings: expect.objectContaining({ cleanup: "clean" }) });
    await waitFor(() => expect(screen.getByText(/BOB types what you said unchanged/)).toBeInTheDocument());
    await userEvent.selectOptions(screen.getByLabelText("Dictation hotkey"), "cmd-shift-space");
    expect(invoke).toHaveBeenCalledWith("dictation_set_settings", {
      settings: expect.objectContaining({ hotkey: { key_code: 49, modifiers: ["cmd", "shift"] } }),
    });
  });

  it("edits vocabulary, replacements, tap-to-toggle and the idle unload", async () => {
    render(<DictationCard />);
    await userEvent.click(await screen.findByText("Vocabulary"));
    await userEvent.type(screen.getByLabelText("New word"), "Dávid, kiroween, Tauri{Enter}");
    expect(invoke).toHaveBeenCalledWith("dictation_set_settings", { settings: expect.objectContaining({ vocabulary: ["Kiroween", "Dávid", "Tauri"] }) });
    await waitFor(() => expect(screen.getByRole("button", { name: "Remove Tauri" })).toBeInTheDocument());
    await userEvent.click(screen.getByRole("button", { name: "Remove Tauri" }));
    expect(invoke).toHaveBeenCalledWith("dictation_set_settings", { settings: expect.objectContaining({ vocabulary: ["Kiroween", "Dávid"] }) });

    await userEvent.click(screen.getByText("Text replacements"));
    await userEvent.click(screen.getByRole("button", { name: "Add replacement" }));
    await userEvent.type(screen.getByLabelText("Replacement 1: when I say"), "my email");
    await userEvent.type(screen.getByLabelText("Replacement 1: type"), "me@example.com");
    await userEvent.click(screen.getByRole("button", { name: "Save replacements" }));
    expect(invoke).toHaveBeenCalledWith("dictation_set_settings", {
      settings: expect.objectContaining({ replacements: [{ from: "my email", to: "me@example.com" }] }),
    });

    await userEvent.click(screen.getByLabelText("Tap to start and stop"));
    expect(invoke).toHaveBeenCalledWith("dictation_set_settings", { settings: expect.objectContaining({ tap_to_toggle: true }) });
    await userEvent.selectOptions(screen.getByLabelText("Unload the speech model"), "15");
    expect(invoke).toHaveBeenCalledWith("dictation_set_settings", { settings: expect.objectContaining({ unload_after_min: 15 }) });
  });

  it("offers the last dictation while it is in memory", async () => {
    view = { ...view, stats: { ...view.stats, last_available: true } };
    const writeText = vi.fn(async () => {});
    Object.assign(navigator, { clipboard: { writeText } });
    invoke.mockImplementation(async (cmd: string) => (cmd === "dictation_get" ? view : cmd === "dictation_last" ? "hello there" : undefined));
    render(<DictationCard />);
    await userEvent.click(await screen.findByRole("button", { name: "Copy last dictation" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("hello there"));
    expect(await screen.findByRole("button", { name: "Copied" })).toBeInTheDocument();
  });

  it("turns dictation off", async () => {
    render(<DictationCard />);
    await userEvent.click(await screen.findByRole("switch", { name: "Voice Dictation" }));
    expect(invoke).toHaveBeenCalledWith("dictation_set_settings", { settings: expect.objectContaining({ enabled: false }) });
  });
});

describe("dictationSummary", () => {
  const v = (patch: Partial<DictationView> = {}, stats: Partial<DictationView["stats"]> = {}): DictationView => ({
    ...view,
    ...patch,
    stats: { ...view.stats, ...stats },
  });
  it("explains what is missing, in order", () => {
    expect(dictationSummary(v({ settings: { ...view.settings, enabled: false } }), "⌥Space")).toBe("Off");
    expect(dictationSummary(v({ models: [model("whisper-large-v3-turbo-q5", false)] }), "⌥Space")).toMatch(/Download/);
    expect(dictationSummary(v({ microphone: "denied" }), "⌥Space")).toMatch(/microphone/);
    expect(dictationSummary(v({}, { model_loaded: false }), "⌥Space")).toMatch(/Loading/);
    expect(dictationSummary(v({}, { model_loaded: false, model_error: "bad file" }), "⌥Space")).toMatch(/bad file/);
    expect(dictationSummary(v({}, { model_loaded: false, idle_unloaded: true }), "⌥Space")).toMatch(/Resting/);
    expect(dictationSummary(v({ settings: { ...view.settings, tap_to_toggle: true } }), "⌥Space")).toMatch(/tap it to start/);
  });
});

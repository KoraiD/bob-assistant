import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { acceptanceRate, minutesLabel, type Day, type UsageView } from "../lib/usage";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));

import { StatsPanel } from "./StatsPanel";

const zero: Day = { ghost_words: 0, ghost_accepts: 0, ghost_shown: 0, dictated_words: 0, dictations: 0, dictated_ms: 0, rewrites: 0, rewrite_failures: 0, voice_edits: 0 };
const day = (p: Partial<Day>): Day => ({ ...zero, ...p });
let usage: UsageView;

beforeEach(() => {
  const recent: [string, Day][] = Array.from({ length: 14 }, (_, i) => [`2026-11-${String(9 + i).padStart(2, "0")}`, i === 13 ? day({ ghost_words: 12, dictated_words: 30 }) : zero]);
  usage = {
    today: day({ ghost_words: 12, dictated_words: 30 }),
    week: day({ ghost_words: 12, dictated_words: 30 }),
    all_time: day({ ghost_words: 50, ghost_accepts: 20, ghost_shown: 80, dictated_words: 150, dictations: 9, dictated_ms: 95_000, rewrites: 4, voice_edits: 2, rewrite_failures: 1 }),
    recent,
    minutes_saved_week: 42 / 40,
    minutes_saved_all: 200 / 40,
    since: "2026-11-01",
  };
  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "usage_get") return usage;
    if (cmd === "usage_clear") {
      usage = { ...usage, today: zero, week: zero, all_time: zero, since: null, minutes_saved_all: 0, minutes_saved_week: 0 };
      return undefined;
    }
    throw new Error("not mocked");
  });
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
});
afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

describe("StatsPanel", () => {
  it("shows words typed for you, time saved, the chart and per-feature counts", async () => {
    render(<StatsPanel />);
    expect(await screen.findByTestId("stat-today")).toHaveTextContent("42");
    expect(screen.getByTestId("stat-all")).toHaveTextContent("Since 2026-11-01");
    expect(screen.getByTestId("stat-all")).toHaveTextContent("about 5 min of typing saved");
    expect(screen.getByRole("img", { name: /last 14 days: 42 in total/ })).toBeInTheDocument();
    expect(screen.getByTestId("stat-ghost")).toHaveTextContent("50 words from 20 accepted suggestions · 25% of 80 shown");
    expect(screen.getByTestId("stat-dictation")).toHaveTextContent("150 words in 9 dictations · 2 min of speech");
    expect(screen.getByTestId("stat-selection")).toHaveTextContent("4 rewrites · 2 voice edits · 1 did not finish");
  });

  it("clears only after confirmation", async () => {
    render(<StatsPanel />);
    await userEvent.click(await screen.findByRole("button", { name: "Clear statistics…" }));
    expect(invoke).not.toHaveBeenCalledWith("usage_clear");
    await userEvent.click(within(screen.getByRole("group", { name: "Confirm clearing statistics" })).getByRole("button", { name: "Clear" }));
    expect(invoke).toHaveBeenCalledWith("usage_clear");
    await waitFor(() => expect(screen.getByTestId("stat-all")).toHaveTextContent("All time"));
  });
});

describe("usage helpers", () => {
  it("formats minutes and acceptance", () => {
    expect(minutesLabel(0.4)).toBe("less than a minute");
    expect(minutesLabel(12.4)).toBe("12 min");
    expect(minutesLabel(60)).toBe("1 h");
    expect(minutesLabel(65)).toBe("1 h 5 min");
    expect(acceptanceRate(zero)).toBeNull();
    expect(acceptanceRate(day({ ghost_shown: 3, ghost_accepts: 1 }))).toBe(33);
  });
});

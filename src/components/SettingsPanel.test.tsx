import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { auditLabel, mcpSnippet, type GeneralView } from "../lib/general";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));

import { SettingsPanel } from "./SettingsPanel";

let general: GeneralView;
beforeEach(() => {
  general = {
    show_in_dock: true,
    open_window_at_launch: true,
    login_item: "disabled",
    pause: { paused: false, until: null },
    log_level: "info",
    log_dir: "/Users/me/Library/Logs/BOB",
    mcp_enabled: false,
    mcp_command: "/Applications/BOB.app/Contents/MacOS/bob-mcp",
  };
  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "general_get":
        return general;
      case "mcp_set_enabled":
        general = { ...general, mcp_enabled: args!.on as boolean };
        return undefined;
      case "general_set_log_level":
        general = { ...general, log_level: args!.level as GeneralView["log_level"] };
        return undefined;
      case "audit_recent":
        return args!.prefix === "data."
          ? [{ at: 1764000000000, kind: "data.sent", detail: { feature: "selection", brain: "openai", cloud: true, chars: 420, personal_context: false } }]
          : [
              { at: 1764000000000, kind: "mcp.call", detail: { tool: "bob_rewrite", ok: true } },
              { at: 1764000001000, kind: "settings.personalize", detail: { changed: { tone: "friendly", style_notes: "(changed)" } } },
            ];
      case "logs_tail":
        return [{ at: 1764000000000, level: "INFO", target: "bob_lib", message: "built-in brain: ready" }];
      default:
        return undefined;
    }
  });
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
});
afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

const panel = () => <SettingsPanel build="BOB 0.1.0" theme="dark" onTheme={() => {}} onTour={() => {}} />;

describe("SettingsPanel", () => {
  it("allows AI agents and shows the MCP configuration", async () => {
    render(panel());
    const sw = await screen.findByRole("switch", { name: "Allow AI agents to use BOB" });
    expect(sw).toHaveAttribute("aria-checked", "false");
    expect(screen.queryByTestId("mcp-snippet")).not.toBeInTheDocument();
    await userEvent.click(sw);
    expect(invoke).toHaveBeenCalledWith("mcp_set_enabled", { on: true });
    expect(await screen.findByTestId("mcp-snippet")).toHaveTextContent("/Applications/BOB.app/Contents/MacOS/bob-mcp");
  });

  it("changes the log level, shows log lines and filters the activity list", async () => {
    render(panel());
    await userEvent.selectOptions(await screen.findByLabelText("Log detail level"), "debug");
    expect(invoke).toHaveBeenCalledWith("general_set_log_level", { level: "debug" });
    await userEvent.click(screen.getByRole("button", { name: "Show" }));
    expect(await screen.findByTestId("log-lines")).toHaveTextContent("built-in brain: ready");
    const list = await screen.findByTestId("audit-list");
    await waitFor(() => expect(list).toHaveTextContent("AI agent used bob_rewrite"));
    expect(list).toHaveTextContent("settings personalize: tone, style_notes");
    await userEvent.click(within(screen.getByRole("radiogroup", { name: "Activity filter" })).getByRole("radio", { name: "Data sent" }));
    await waitFor(() => expect(screen.getByTestId("audit-list")).toHaveTextContent("Text sent to cloud brain openai for selection (420 characters)"));
  });
});

describe("general helpers", () => {
  it("labels audit entries without any text content", () => {
    expect(auditLabel({ at: 0, kind: "brain.saved", detail: { brain: "x", key: "set" } })).toBe("Brain saved: x (API key set)");
    expect(auditLabel({ at: 0, kind: "security.peer_rejected", detail: {} })).toMatch(/another user/);
    expect(auditLabel({ at: 0, kind: "data.sent", detail: { feature: "try_it", brain: "openai", cloud: true, chars: 12 } })).toBe(
      "Text sent to cloud brain openai for try it (12 characters)",
    );
    expect(auditLabel({ at: 0, kind: "dictation_cleanup.withheld", detail: { brain: "openai", chars: 40 } })).toMatch(/not sent to brain openai/);
    expect(auditLabel({ at: 0, kind: "model.mismatch", detail: { bytes: 1, deleted: true } })).toMatch(/deleted/);
    expect(JSON.parse(mcpSnippet(null)).mcpServers.bob.command).toMatch(/bob-mcp$/);
  });
});

import { render, screen } from "@testing-library/react";
import App, { CAPABILITIES } from "./App";
import { BROWSER_APP_INFO, getAppInfo, isTauri } from "./lib/appInfo";

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
    expect(screen.getByRole("img", { name: /friendly ghost/i })).toBeInTheDocument();
  });
});

describe("appInfo", () => {
  it("falls back to browser info outside Tauri", async () => {
    expect(isTauri()).toBe(false);
    await expect(getAppInfo()).resolves.toEqual(BROWSER_APP_INFO);
  });
});

import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { GHOST_LINES, GhostMark, lookOffset } from "./GhostMark";

describe("GhostMark", () => {
  it("is a plain labelled image unless interactive", () => {
    render(<GhostMark />);
    expect(screen.getByRole("img", { name: "BOB the friendly ghost" })).toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });

  it("looks at the pointer, within the eye", () => {
    expect(lookOffset(0, 0)).toEqual({ x: 0, y: 0 });
    const far = lookOffset(400, 0);
    expect(far.x).toBeCloseTo(1.9);
    expect(far.y).toBeCloseTo(0);
    const near = lookOffset(0, -20);
    expect(near.y).toBeCloseTo(-0.5);
    const diag = lookOffset(-300, 300);
    expect(Math.hypot(diag.x, diag.y)).toBeLessThanOrEqual(1.9 + 1e-9);
  });

  it("follows the mouse, blushes on hover and talks when clicked", async () => {
    vi.spyOn(window, "requestAnimationFrame").mockImplementation((cb) => {
      cb(0);
      return 1;
    });
    render(<GhostMark interactive size={48} />);
    const button = screen.getByRole("button", { name: /friendly ghost/ });
    const pupils = screen.getAllByTestId("ghost-pupil");
    act(() => {
      fireEvent.mouseMove(window, { clientX: 500, clientY: 0 });
    });
    expect(Number(pupils[0].getAttribute("cx"))).toBeGreaterThan(24);
    fireEvent.pointerEnter(button);
    expect(document.querySelector(".ghost-blush")).toHaveStyle({ opacity: "0.55" });
    await userEvent.click(button);
    expect(screen.getByText(GHOST_LINES[0])).toBeInTheDocument();
    await userEvent.click(button);
    expect(screen.getByText(GHOST_LINES[1])).toBeInTheDocument();
    vi.restoreAllMocks();
  });
});

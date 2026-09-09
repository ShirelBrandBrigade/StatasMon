import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import { StatusDot } from "./StatusDot";

test("StatusDot always carries a label and paints from the tone token", () => {
  render(<StatusDot tone="bad" label="Recording" glow />);
  const dot = screen.getByRole("img", { name: "Recording" });
  expect(dot).toHaveAttribute("data-tone", "bad");
  expect(dot).toHaveClass("is-glow");
  expect(dot.style.background).toBe("var(--red)");
});

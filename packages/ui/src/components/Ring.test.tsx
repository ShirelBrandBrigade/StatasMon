import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import { Ring } from "./Ring";

test("Ring draws a dasharray arc proportional to the value and is labelled", () => {
  render(<Ring value={38} name="CPU" />);
  const svg = screen.getByRole("img", { name: "CPU 38%" });
  expect(svg).toHaveAttribute("width", "78");
  const arc = screen.getByTestId("ring-arc");
  const [dash, circ] = (arc.getAttribute("stroke-dasharray") ?? "").split(" ").map(Number);
  expect(circ).toBeCloseTo(2 * Math.PI * 34, 1);
  expect(dash / circ).toBeCloseTo(0.38, 2);
  expect(arc).toHaveAttribute("stroke", "var(--cyan)");
  expect(arc).toHaveAttribute("stroke-width", "8");
});

test("Ring clamps and honours series and null values", () => {
  const { rerender } = render(<Ring value={140} name="Memory" series="vio" />);
  expect(screen.getByRole("img", { name: "Memory 100%" })).toBeInTheDocument();
  expect(screen.getByTestId("ring-arc")).toHaveAttribute("stroke", "var(--vio)");
  rerender(<Ring value={null} name="Memory" />);
  expect(screen.getByRole("img", { name: "Memory —" })).toBeInTheDocument();
  expect(screen.queryByTestId("ring-arc")).toBeNull();
});

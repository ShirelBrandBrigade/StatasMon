import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import { Dash } from "./Dash";

test("Dash renders an em dash with the reason as tooltip and accessible name", () => {
  render(<Dash reason="iOS does not report per-process network" />);
  const el = screen.getByTitle("iOS does not report per-process network");
  expect(el).toHaveTextContent("—");
  expect(el).toHaveAttribute("aria-label", "Unavailable: iOS does not report per-process network");
  expect(el).toHaveClass("mono");
});

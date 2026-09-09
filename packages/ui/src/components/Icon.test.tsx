import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import { ICON_NAMES, Icon } from "./Icon";

test("every icon name renders a 24-grid stroke SVG", () => {
  for (const name of ICON_NAMES) {
    const { container, unmount } = render(<Icon name={name} />);
    const svg = container.querySelector("svg");
    expect(svg).toHaveAttribute("viewBox", "0 0 24 24");
    expect(svg).toHaveAttribute("stroke-width", "2");
    expect(svg).toHaveAttribute("data-icon", name);
    expect(svg?.childElementCount).toBeGreaterThan(0);
    unmount();
  }
  expect(ICON_NAMES).toHaveLength(22);
});

test("Icon is decorative unless labelled", () => {
  render(<Icon name="warning" label="Warning" size={20} />);
  const svg = screen.getByRole("img", { name: "Warning" });
  expect(svg).toHaveAttribute("width", "20");
  const { container } = render(<Icon name="check" />);
  expect(container.querySelector("svg")).toHaveAttribute("aria-hidden", "true");
});

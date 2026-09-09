import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import { GALLERY_SECTIONS, Gallery } from "./Gallery";

test("Gallery renders a section heading for every primitive", () => {
  render(<Gallery />);
  for (const name of GALLERY_SECTIONS) {
    expect(screen.getByRole("heading", { level: 2, name })).toBeInTheDocument();
  }
  expect(GALLERY_SECTIONS).toHaveLength(13);
});

test("Gallery contains no emoji", () => {
  const { container } = render(<Gallery />);
  expect(container.textContent).not.toMatch(/\p{Extended_Pictographic}/u);
});

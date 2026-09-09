import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import { AppTile } from "./AppTile";

test("AppTile marks the app under test and carries its label", () => {
  render(<AppTile me label="FieldKit" />);
  const tile = screen.getByRole("img", { name: "FieldKit" });
  expect(tile).toHaveClass("sm-tile", "is-me");
  expect(tile).toHaveAttribute("data-me", "true");
});

test("AppTile without a label is decorative", () => {
  const { container } = render(<AppTile />);
  const tile = container.firstElementChild;
  expect(tile).toHaveAttribute("aria-hidden", "true");
  expect(tile).not.toHaveClass("is-me");
});

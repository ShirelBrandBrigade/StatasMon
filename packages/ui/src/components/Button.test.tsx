import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { Button, IconButton } from "./Button";

test("Button renders variants, sizes and a leading icon", () => {
  const onClick = vi.fn();
  render(
    <Button variant="primary" size="sm" icon="download" onClick={onClick}>
      Export
    </Button>,
  );
  const btn = screen.getByRole("button", { name: "Export" });
  expect(btn).toHaveClass("sm-btn", "is-primary", "is-sm");
  expect(btn).toHaveAttribute("type", "button");
  expect(btn.querySelector("svg[data-icon='download']")).not.toBeNull();
  fireEvent.click(btn);
  expect(onClick).toHaveBeenCalledOnce();
});

test("IconButton exposes its label as the accessible name", () => {
  render(<IconButton icon="close" label="Close" />);
  const btn = screen.getByRole("button", { name: "Close" });
  expect(btn).toHaveClass("sm-icon-btn");
  expect(btn).toHaveAttribute("title", "Close");
});

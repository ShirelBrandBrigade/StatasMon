import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import { App } from "./App";

test("renders the debug panel shell", () => {
  render(<App />);
  expect(screen.getByRole("heading", { name: "StatasMon" })).toBeInTheDocument();
  expect(screen.getByText(/Debug panel/)).toBeInTheDocument();
});

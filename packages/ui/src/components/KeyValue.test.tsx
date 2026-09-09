import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import { KeyValue } from "./KeyValue";

test("KeyValue renders label/value rows with mono values and tone marks", () => {
  render(
    <KeyValue
      rows={[
        { label: "Voltage", value: "4.21 V" },
        { label: "Health", value: "96%", tone: "ok" },
      ]}
    />,
  );
  expect(screen.getByText("Voltage")).toBeInTheDocument();
  const v = screen.getByText("4.21 V");
  expect(v).toHaveClass("mono");
  expect(v.tagName).toBe("DD");
  expect(screen.getByRole("img", { name: "ok" })).toHaveAttribute("data-tone", "ok");
});

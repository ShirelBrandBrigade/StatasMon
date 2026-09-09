import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { Segment } from "./Segment";

const options = [
  { value: "live", label: "Live" },
  { value: "5m", label: "5 min" },
  { value: "15m", label: "15 min" },
] as const;

test("Segment is a labelled radio group that marks the selected option and emits changes", () => {
  const onChange = vi.fn();
  render(<Segment name="Range" options={options} value="5m" onChange={onChange} />);
  expect(screen.getByRole("group", { name: "Range" })).toHaveClass("sm-seg");
  const selected = screen.getByRole("radio", { name: "5 min" });
  expect(selected).toBeChecked();
  expect(selected.closest("label")).toHaveAttribute("data-checked", "true");
  expect(screen.getByRole("radio", { name: "Live" })).not.toBeChecked();
  fireEvent.click(screen.getByRole("radio", { name: "15 min" }));
  expect(onChange).toHaveBeenLastCalledWith("15m");
  expect(onChange).toHaveBeenCalledOnce();
});

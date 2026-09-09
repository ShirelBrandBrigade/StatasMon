import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import { MetricFigure, splitFigure } from "./MetricFigure";

test("splitFigure separates number and unit from formatValue output", () => {
  expect(splitFigure("24.6 Mb/s")).toEqual({ number: "24.6", unit: "Mb/s" });
  expect(splitFigure("38%")).toEqual({ number: "38", unit: "%" });
  expect(splitFigure("1 842")).toEqual({ number: "1 842", unit: "" });
  expect(splitFigure("4.21 V")).toEqual({ number: "4.21", unit: "V" });
  expect(splitFigure("Yes")).toBeNull();
});

test("MetricFigure renders the number in mono with a small unit and caption", () => {
  render(
    <MetricFigure
      meta={{ unit: "mbps" }}
      value={24.6}
      suffix="down"
      caption="1 842 pkt/s"
      series="cyan"
    />,
  );
  const num = screen.getByTestId("figure-number");
  expect(num).toHaveTextContent("24.6");
  expect(num.parentElement).toHaveClass("mono");
  expect(screen.getByText("Mb/s down")).toHaveClass("sm-figure-unit");
  expect(screen.getByText("1 842 pkt/s")).toHaveClass("mono");
});

test("MetricFigure shows a Dash with a reason for missing values", () => {
  render(
    <MetricFigure meta={{ unit: "celsius" }} value={null} unavailableReason="Android hides this" />,
  );
  expect(screen.getByTitle("Android hides this")).toHaveTextContent("—");
});

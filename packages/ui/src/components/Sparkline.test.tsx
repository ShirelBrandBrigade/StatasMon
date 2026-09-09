import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import { Sparkline, sparklinePoints } from "./Sparkline";

test("sparklinePoints scales values to the box", () => {
  const pts = sparklinePoints([0, 10, 5], 100, 50);
  expect(pts).toHaveLength(3);
  expect(pts[0][0]).toBe(0);
  expect(pts[2][0]).toBe(100);
  expect(pts[0][1]).toBe(46); // min sits at height - PAD
  expect(pts[1][1]).toBe(4); // max sits at PAD
  expect(sparklinePoints([], 100, 50)).toEqual([]);
  expect(sparklinePoints([3, 3], 100, 50)[0][1]).toBe(25); // flat line guard
});

test("Sparkline renders line, gradient fill and endpoint dot", () => {
  render(
    <Sparkline values={[1, 2, 3]} width={90} height={30} series="amber" fill endpoint name="SoC" />,
  );
  expect(screen.getByRole("img", { name: "SoC" })).toBeInTheDocument();
  expect(screen.getByTestId("spark-line")).toHaveAttribute("stroke", "var(--amber)");
  expect(screen.getByTestId("spark-area").getAttribute("fill")).toMatch(/^url\(#sm-spark-/);
  const dot = screen.getByTestId("spark-endpoint");
  expect(dot).toHaveAttribute("cx", "90");
  expect(dot).toHaveAttribute("stroke", "var(--bg)");
});

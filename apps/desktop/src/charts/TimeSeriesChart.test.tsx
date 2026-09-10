import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { act, render } from "@testing-library/react";
import type uPlot from "uplot";
import { beforeAll, expect, test } from "vitest";
import { applyTokens, installCanvasStub } from "./testing/jsdomCanvas";
import type { ChartData } from "./useSeriesData";

// uPlot touches matchMedia and the canvas at import time, so the stub goes first.
const canvas = installCanvasStub();
const { formatClock, TimeSeriesChart } = await import("./TimeSeriesChart");

const TOKENS_CSS = resolve(__dirname, "../../../../packages/ui/src/tokens.css");
const SERIES = [
  { id: "a", label: "Down", color: "var(--cyan)", fill: true, endpoint: true },
  { id: "b", label: "Up", color: "var(--vio)" },
];
const T0 = Date.UTC(2026, 8, 9, 12, 0, 0);

function stream(n: number, offset = 0): ChartData {
  const t: number[] = [];
  const a: number[] = [];
  const b: number[] = [];
  for (let i = 0; i < n; i += 1) {
    t.push(T0 + (i + offset) * 1000);
    a.push(1e6 + i);
    b.push(2e5 + i);
  }
  return { t, v: [a, b] };
}

const flush = () => act(async () => {});

beforeAll(() => {
  applyTokens(readFileSync(TOKENS_CSS, "utf8"));
});

test("mounts one uPlot instance, updates through setData and destroys on unmount", async () => {
  const ref: { current: uPlot | null } = { current: null };
  const { container, rerender, unmount } = render(
    <TimeSeriesChart
      series={SERIES}
      data={stream(10)}
      height={90}
      windowMs={60_000}
      instanceRef={ref}
      unit="b/s"
    />,
  );
  await flush();
  const u = ref.current;
  expect(u).not.toBeNull();
  if (!u) return;
  expect(container.querySelectorAll(".uplot").length).toBe(1);
  expect(container.querySelector(".u-legend")).toBeNull();
  expect(u.series.length).toBe(3);
  expect(u.series[1].width).toBe(2);
  expect(u.data[0].length).toBe(10);
  expect(u.data[0][0]).toBe(T0 / 1000);
  // Host is the accessible image; the label comes from the series and unit.
  expect(container.querySelector('[role="img"]')?.getAttribute("aria-label")).toBe(
    "Down and Up in b/s",
  );

  rerender(
    <TimeSeriesChart
      series={SERIES}
      data={stream(11)}
      height={90}
      windowMs={60_000}
      instanceRef={ref}
      unit="b/s"
    />,
  );
  await flush();
  expect(ref.current).toBe(u); // same instance: no re-create on data change
  expect(u.data[0].length).toBe(11);
  // x scale follows the newest point; with only 10 s of data the window is
  // no wider than the data plus the MIN_SPAN_S floor (30 s), not the full 60 s.
  expect(u.scales.x.max).toBeCloseTo((T0 + 10_000) / 1000);
  expect(u.scales.x.min).toBeCloseTo((T0 + 10_000 - 30_000) / 1000);
  // y scale starts at 0 with headroom above the max.
  expect(u.scales.y.min).toBe(0);
  expect(u.scales.y.max).toBeGreaterThan(1e6 + 10);

  unmount();
  expect(ref.current).toBeNull();
  expect(container.querySelector(".uplot")).toBeNull();
});

test("draws an endpoint dot for the flagged series only and shows the tooltip on hover", async () => {
  const ref: { current: uPlot | null } = { current: null };
  const yFormat = (v: number) => `${Math.round(v)} b/s`;
  const { container } = render(
    <TimeSeriesChart
      series={SERIES}
      data={stream(5)}
      height={90}
      windowMs={60_000}
      instanceRef={ref}
      yFormat={yFormat}
    />,
  );
  await flush();
  const u = ref.current;
  if (!u) throw new Error("no instance");

  canvas.reset();
  u.redraw();
  await flush();
  // One arc per endpoint series (the hover points are DOM, not canvas).
  expect(canvas.calls.filter((c) => c === "arc").length).toBe(1);

  const tip = container.querySelector(".sm-chart-tip") as HTMLElement | null;
  expect(tip).not.toBeNull();
  expect(tip?.hidden).toBe(true);
  act(() => {
    u.setCursor({ left: u.width / 2, top: 10 });
  });
  expect(tip?.hidden).toBe(false);
  const rows = Array.from(tip?.querySelectorAll(".sm-chart-tip-row") ?? []);
  expect(rows.map((r) => r.querySelector("span")?.textContent)).toEqual(["Down", "Up"]);
  const values = rows.map((r) => r.querySelector(".sm-chart-tip-value")?.textContent);
  expect(values[0]).toMatch(/^\d+ b\/s$/);
  expect(values[1]).toMatch(/^\d+ b\/s$/);
  expect(tip?.querySelector(".sm-chart-tip-time")?.textContent).toMatch(/^\d\d:\d\d:\d\d$/);

  act(() => {
    u.setCursor({ left: -10, top: -10 });
  });
  expect(tip?.hidden).toBe(true);
});

test("formatClock renders HH:MM and HH:MM:SS in local time", () => {
  const d = new Date(2026, 8, 9, 14, 7, 9);
  expect(formatClock(d.getTime() / 1000)).toBe("14:07");
  expect(formatClock(d.getTime() / 1000, true)).toBe("14:07:09");
});

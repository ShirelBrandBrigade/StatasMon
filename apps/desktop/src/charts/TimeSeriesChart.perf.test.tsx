import { act, render } from "@testing-library/react";
import type uPlot from "uplot";
import { expect, test } from "vitest";
import { installCanvasStub } from "./testing/jsdomCanvas";

// uPlot touches matchMedia and the canvas at import time, so the stub goes first.
installCanvasStub();
const { TimeSeriesChart } = await import("./TimeSeriesChart");
const { makeStream, slide } = await import("./perfHarness");
const { toAlignedData } = await import("./useSeriesData");

/**
 * Budget from docs/PLAN.md 1.3: 3,600 points × 2 series appended at 1 Hz with
 * under 4 ms of main-thread work per update. Under jsdom the canvas is a stub,
 * so this measures everything except rasterization: React re-render, effect,
 * ms → s conversion, uPlot's setData, scale ranging, path building and the
 * draw hooks. `charts/perfHarness.ts` gives the in-browser number.
 */
const POINTS = 3600;
const UPDATES = 60;
const WARMUP = 5;
const BUDGET_MS = 4;
const SERIES = [
  { id: "a", label: "Down", color: "var(--cyan)", fill: true, endpoint: true },
  { id: "b", label: "Up", color: "var(--vio)" },
];
const T0 = Date.UTC(2026, 8, 9, 12, 0, 0) - POINTS * 1000;

/** The reporter hides console output; write the measurement straight to stdout. */
function report(line: string) {
  process.stdout.write(`chart perf (jsdom): ${line}\n`);
}

function stats(times: number[]) {
  const sorted = [...times].sort((a, b) => a - b);
  const avg = times.reduce((s, x) => s + x, 0) / times.length;
  return { avg, p95: sorted[Math.floor(sorted.length * 0.95)], max: sorted[sorted.length - 1] };
}

test(`appends ${UPDATES} updates to ${POINTS} × 2 points under ${BUDGET_MS} ms each (React path)`, async () => {
  const ref: { current: uPlot | null } = { current: null };
  const data = makeStream(POINTS, 0, T0);
  const props = { series: SERIES, height: 90, windowMs: POINTS * 1000, instanceRef: ref };
  const { rerender } = render(<TimeSeriesChart {...props} data={data} />);
  await act(async () => {});
  expect(ref.current?.data[0].length).toBe(POINTS);

  const times: number[] = [];
  for (let k = 1; k <= WARMUP + UPDATES; k += 1) {
    // What useMetricSeries hands us each tick: the window slid by one sample.
    const next = slide(data, POINTS + k);
    const start = performance.now();
    await act(async () => {
      rerender(<TimeSeriesChart {...props} data={{ t: next.t, v: next.v }} />);
    });
    const elapsed = performance.now() - start;
    if (k > WARMUP) times.push(elapsed);
  }
  const { avg, p95, max } = stats(times);
  expect(ref.current?.data[0].length).toBe(POINTS);
  expect(ref.current?.data[0][POINTS - 1]).toBeCloseTo(
    (T0 + (POINTS - 1 + UPDATES + WARMUP) * 1000) / 1000,
  );
  report(
    `React render + setData + draw: avg ${avg.toFixed(3)} ms/update · p95 ${p95.toFixed(3)} · max ${max.toFixed(3)} · ${UPDATES} updates × ${POINTS} pts × 2 series`,
  );
  expect(avg).toBeLessThan(BUDGET_MS);
});

test(`conversion + setData + deferred draw alone stay under ${BUDGET_MS} ms per update`, async () => {
  const ref: { current: uPlot | null } = { current: null };
  const data = makeStream(POINTS, 0, T0);
  render(
    <TimeSeriesChart
      series={SERIES}
      data={data}
      height={90}
      windowMs={POINTS * 1000}
      instanceRef={ref}
    />,
  );
  await act(async () => {});
  const u = ref.current;
  if (!u) throw new Error("chart did not mount");

  let aligned = u.data;
  const times: number[] = [];
  for (let k = 1; k <= WARMUP + UPDATES; k += 1) {
    const next = slide(data, POINTS + k);
    const start = performance.now();
    aligned = toAlignedData(next, aligned);
    u.setData(aligned);
    await Promise.resolve(); // uPlot commits (ranges scales, builds paths, draws) in a microtask
    const elapsed = performance.now() - start;
    if (k > WARMUP) times.push(elapsed);
  }
  const { avg, p95, max } = stats(times);
  report(
    `toAlignedData + setData + draw: avg ${avg.toFixed(3)} ms/update · p95 ${p95.toFixed(3)} · max ${max.toFixed(3)}`,
  );
  expect(avg).toBeLessThan(BUDGET_MS);
});

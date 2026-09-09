import { createElement } from "react";
import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import type uPlot from "uplot";
import { TimeSeriesChart } from "./TimeSeriesChart";
import { type ChartData, toAlignedData } from "./useSeriesData";

export interface ChartPerfOptions {
  /** Points per series (default 3,600 = one hour at 1 Hz). */
  points?: number;
  /** Appends to time (default 60). */
  updates?: number;
  width?: number;
  height?: number;
}

export interface ChartPerfResult {
  msPerUpdate: number;
  maxMs: number;
  points: number;
  updates: number;
  /** True when the document was hidden during the run (no frame pacing, no paint). */
  hidden: boolean;
}

/** Deterministic two-series stream; `offset` shifts the window by that many samples. */
export function makeStream(points: number, offset: number, t0: number): ChartData {
  const t = new Array<number>(points);
  const a = new Array<number>(points);
  const b = new Array<number>(points);
  for (let i = 0; i < points; i += 1) {
    const k = i + offset;
    t[i] = t0 + k * 1000;
    a[i] = 15e6 + 10e6 * Math.sin(k / 97) + 2e6 * Math.sin(k / 7);
    b[i] = 2e6 + 1.5e6 * Math.abs(Math.sin(k / 41));
  }
  return { t, v: [a, b] };
}

/** Slides `prev` one sample forward in place (what a ring-buffer window yields at 1 Hz). */
export function slide(prev: ChartData, k: number): ChartData {
  const n = prev.t.length;
  for (let i = 1; i < n; i += 1) {
    prev.t[i - 1] = prev.t[i];
    prev.v[0][i - 1] = prev.v[0][i];
    prev.v[1][i - 1] = prev.v[1][i];
  }
  prev.t[n - 1] = prev.t[n - 2] + 1000;
  prev.v[0][n - 1] = 15e6 + 10e6 * Math.sin(k / 97) + 2e6 * Math.sin(k / 7);
  prev.v[1][n - 1] = 2e6 + 1.5e6 * Math.abs(Math.sin(k / 41));
  return { t: prev.t, v: prev.v };
}

/**
 * A frame when the page is visible. Hidden documents get neither rAF nor
 * (when frozen) timers, so pacing falls back to a microtask there; uPlot's
 * deferred commit runs in a microtask as well, so the measurement still
 * covers conversion, ranging, path building and canvas command issue.
 */
function nextFrame(): Promise<void> {
  return new Promise((r) => {
    if (document.hidden) queueMicrotask(r);
    else requestAnimationFrame(() => r());
  });
}

/**
 * In-browser budget check for docs/PLAN.md 1.3 (3,600 points × 2 series at
 * 1 Hz, < 4 ms of main-thread work per update). Mounts a real chart into
 * `document.body`, then times conversion + `setData` + the deferred draw for
 * each update and removes the chart again. Call from the devtools console of
 * `pnpm web:mock` / `pnpm dev:mock`:
 *
 *   const m = await import("/src/charts/perfHarness.ts"); await m.runChartPerf();
 */
export async function runChartPerf(opts: ChartPerfOptions = {}): Promise<ChartPerfResult> {
  const points = opts.points ?? 3600;
  const updates = opts.updates ?? 60;
  const host = document.createElement("div");
  host.style.cssText = `position:fixed;left:0;top:0;width:${opts.width ?? 720}px;z-index:9999`;
  document.body.appendChild(host);
  const instanceRef: { current: uPlot | null } = { current: null };
  const t0 = Date.now() - points * 1000;
  const data = makeStream(points, 0, t0);
  const root = createRoot(host);
  const times: number[] = [];
  try {
    // Synchronous mount (render + effects) so the harness also works where React's scheduler cannot run.
    flushSync(() => {
      root.render(
        createElement(TimeSeriesChart, {
          series: [
            { id: "a", label: "Down", color: "var(--cyan)", fill: true, endpoint: true },
            { id: "b", label: "Up", color: "var(--vio)" },
          ],
          data,
          height: opts.height ?? 90,
          windowMs: points * 1000,
          instanceRef,
        }),
      );
    });
    for (let tries = 0; !instanceRef.current && tries < 300; tries += 1) await nextFrame();
    const u = instanceRef.current;
    if (!u) throw new Error("chart did not mount");

    let aligned = u.data;
    for (let k = 1; k <= updates; k += 1) {
      const next = slide(data, points + k);
      const start = performance.now();
      aligned = toAlignedData(next, aligned);
      u.setData(aligned);
      await Promise.resolve(); // uPlot commits (draws) in a microtask
      times.push(performance.now() - start);
      await nextFrame();
    }
  } finally {
    flushSync(() => root.unmount());
    host.remove();
  }
  const msPerUpdate = times.reduce((s, x) => s + x, 0) / times.length;
  const maxMs = Math.max(...times);
  const result = { msPerUpdate, maxMs, points, updates, hidden: document.hidden };
  console.info(
    `TimeSeriesChart perf: ${points} × 2 series, ${updates} updates → ${msPerUpdate.toFixed(2)} ms/update (max ${maxMs.toFixed(2)} ms)${result.hidden ? " · document hidden: no paint pacing" : ""}`,
  );
  return result;
}

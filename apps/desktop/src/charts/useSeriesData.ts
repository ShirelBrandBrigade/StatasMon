import type { MetricId } from "@statasmon/protocol";
import { useMemo, useRef } from "react";
import type uPlot from "uplot";
import { useMetricSeries } from "../hooks/useDevice";

/**
 * Chart input: timestamps in ms (oldest → newest) and one value column per
 * series. `NaN` marks a gap. Consumers must treat the arrays as transient:
 * `useSeriesData` reuses them between updates.
 */
export interface ChartData {
  t: number[];
  v: number[][];
}

export interface SeriesInput {
  t: number[];
  v: number[];
}

/**
 * Aligns `inputs` on their timestamps into `out`, reusing `out`'s arrays
 * (only `.length` changes), so a 1 Hz stream allocates nothing here.
 * A timestamp present in only some inputs gets `NaN` in the others.
 */
export function alignSeries(inputs: readonly SeriesInput[], out: ChartData): ChartData {
  const k = inputs.length;
  while (out.v.length < k) out.v.push([]);
  out.v.length = k;
  const t = out.t;

  // Fast path: identical timelines (one MetricBuffer, one window → same `t`).
  const first = inputs[0];
  const n0 = first?.t.length ?? 0;
  let same = true;
  for (let i = 1; i < k && same; i += 1) {
    const ti = inputs[i].t;
    if (ti.length !== n0) {
      same = false;
      break;
    }
    for (let j = 0; j < n0; j += 1) {
      if (ti[j] !== first.t[j]) {
        same = false;
        break;
      }
    }
  }
  if (same) {
    t.length = n0;
    for (let j = 0; j < n0; j += 1) t[j] = first.t[j];
    for (let i = 0; i < k; i += 1) {
      const src = inputs[i].v;
      const dst = out.v[i];
      dst.length = n0;
      for (let j = 0; j < n0; j += 1) dst[j] = src[j];
    }
    return out;
  }

  // General path: k-way merge of sorted timelines.
  const pos = new Array<number>(k).fill(0);
  let n = 0;
  for (;;) {
    let next = Number.POSITIVE_INFINITY;
    for (let i = 0; i < k; i += 1) {
      const ti = inputs[i].t;
      if (pos[i] < ti.length && ti[pos[i]] < next) next = ti[pos[i]];
    }
    if (next === Number.POSITIVE_INFINITY) break;
    t[n] = next;
    for (let i = 0; i < k; i += 1) {
      const inp = inputs[i];
      if (pos[i] < inp.t.length && inp.t[pos[i]] === next) {
        out.v[i][n] = inp.v[pos[i]];
        pos[i] += 1;
      } else {
        out.v[i][n] = Number.NaN;
      }
    }
    n += 1;
  }
  t.length = n;
  for (let i = 0; i < k; i += 1) out.v[i].length = n;
  return out;
}

/**
 * uPlot's aligned form `[t / 1000, v1, v2, …]` (seconds; `NaN` → `null`).
 * Reuses `out` when given so the per-update cost is a copy, not an allocation.
 */
export function toAlignedData(data: ChartData, out?: uPlot.AlignedData): uPlot.AlignedData {
  const n = data.t.length;
  const cols = data.v.length + 1;
  const res: (number | null)[][] = (out as (number | null)[][] | undefined) ?? [];
  while (res.length < cols) res.push([]);
  res.length = cols;
  const ts = res[0];
  ts.length = n;
  for (let j = 0; j < n; j += 1) ts[j] = data.t[j] / 1000;
  for (let i = 0; i < data.v.length; i += 1) {
    const src = data.v[i];
    const dst = res[i + 1];
    dst.length = n;
    for (let j = 0; j < n; j += 1) {
      const x = src[j];
      dst[j] = Number.isFinite(x) ? x : null;
    }
  }
  return res as unknown as uPlot.AlignedData;
}

/**
 * Two metrics of one device as chart data, limited to the selected range.
 * Returns a new wrapper object per update (so effects keyed on it fire) around
 * arrays that are reused in place.
 */
export function useSeriesData(deviceId: string | undefined, a: MetricId, b: MetricId): ChartData {
  const sa = useMetricSeries(deviceId, a);
  const sb = useMetricSeries(deviceId, b);
  const out = useRef<ChartData>({ t: [], v: [[], []] });
  return useMemo(() => {
    const o = alignSeries([sa, sb], out.current);
    return { t: o.t, v: o.v };
  }, [sa, sb]);
}

import { act, renderHook } from "@testing-library/react";
import { beforeEach, expect, test } from "vitest";
import { dispatch } from "../bridge/dispatch";
import { synthesize } from "../bridge/synthetic";
import { useAppStore } from "../store/app";
import { telemetry } from "../store/telemetry";
import { alignSeries, type ChartData, toAlignedData, useSeriesData } from "./useSeriesData";

beforeEach(() => {
  useAppStore.getState().reset();
  telemetry.reset();
});

test("alignSeries fast path copies identical timelines into the reused arrays", () => {
  const out: ChartData = { t: [9, 9, 9, 9, 9], v: [[1], []] };
  const t = out.t;
  const v0 = out.v[0];
  const res = alignSeries(
    [
      { t: [1000, 2000, 3000], v: [1, 2, 3] },
      { t: [1000, 2000, 3000], v: [10, 20, 30] },
    ],
    out,
  );
  expect(res).toBe(out);
  expect(res.t).toBe(t);
  expect(res.v[0]).toBe(v0);
  expect(res.t).toEqual([1000, 2000, 3000]);
  expect(res.v).toEqual([
    [1, 2, 3],
    [10, 20, 30],
  ]);
});

test("alignSeries merges different timelines with NaN gaps", () => {
  const out: ChartData = { t: [], v: [] };
  const res = alignSeries(
    [
      { t: [1000, 3000], v: [1, 3] },
      { t: [2000, 3000, 4000], v: [20, 30, 40] },
    ],
    out,
  );
  expect(res.t).toEqual([1000, 2000, 3000, 4000]);
  expect(res.v[0]).toEqual([1, Number.NaN, 3, Number.NaN]);
  expect(res.v[1]).toEqual([Number.NaN, 20, 30, 40]);
  // Empty inputs give empty output without throwing.
  expect(alignSeries([], out).t).toEqual([]);
  expect(
    alignSeries(
      [
        { t: [], v: [] },
        { t: [], v: [] },
      ],
      out,
    ).v,
  ).toEqual([[], []]);
});

test("toAlignedData converts ms → s, NaN → null, and reuses the output arrays", () => {
  const data: ChartData = {
    t: [1000, 2000, 3000],
    v: [
      [1, Number.NaN, 3],
      [4, 5, 6],
    ],
  };
  const first = toAlignedData(data);
  expect(first).toEqual([
    [1, 2, 3],
    [1, null, 3],
    [4, 5, 6],
  ]);
  const t = first[0];
  const s1 = first[1];
  const second = toAlignedData(
    {
      t: [4000, 5000],
      v: [
        [7, 8],
        [9, 10],
      ],
    },
    first,
  );
  expect(second).toBe(first);
  expect(second[0]).toBe(t);
  expect(second[1]).toBe(s1);
  expect(second).toEqual([
    [4, 5],
    [7, 8],
    [9, 10],
  ]);
});

test("useSeriesData aligns two metrics of a device and reuses its arrays across ticks", async () => {
  act(() => {
    for (const { message } of synthesize(3000)) dispatch(message);
  });
  const { result } = renderHook(() => useSeriesData("fake:iphone", "net.rx_bps", "net.tx_bps"));
  const a = result.current;
  expect(a.t.length).toBe(3);
  expect(a.v.length).toBe(2);
  expect(a.v[0].every((x) => Number.isFinite(x))).toBe(true);
  const rx = telemetry.metricsFor("fake:iphone").window("net.rx_bps");
  expect(a.t).toEqual(rx.t);
  expect(a.v[0]).toEqual(rx.v);

  await act(async () => {
    dispatch({
      type: "samples",
      device_id: "fake:iphone",
      ts_ms: a.t[2] + 1000,
      values: [
        { id: "net.rx_bps", value: 1 },
        { id: "net.tx_bps", value: 2 },
      ],
    });
    await new Promise((r) => requestAnimationFrame(r));
  });
  const b = result.current;
  expect(b).not.toBe(a);
  expect(b.t).toBe(a.t);
  expect(b.v[0]).toBe(a.v[0]);
  expect(b.t.length).toBe(4);
  expect(b.v[0][3]).toBe(1);
  expect(b.v[1][3]).toBe(2);
});

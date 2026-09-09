import { act, renderHook } from "@testing-library/react";
import { beforeEach, expect, test } from "vitest";
import { proc, processes, seed } from "../../overview/cards/testkit";
import {
  PROCESS_HISTORY_POINTS,
  PROCESS_HISTORY_RINGS,
  processHistorySize,
  recordProcessRow,
  resetProcessHistory,
  useProcessHistory,
} from "./useProcessHistory";

const ID = "fake:iphone";

beforeEach(() => {
  seed(0);
  resetProcessHistory();
});

const table = (cpu: number) => [
  proc({ pid: 7, name: "a", cpu_pct: cpu, mem_bytes: cpu * 1e6, net_bps: cpu * 10 }),
  proc({ pid: 8, name: "b", cpu_pct: 1 }),
];

test("appends one point per distinct table and returns copies", () => {
  const t1 = table(10);
  const h1 = recordProcessRow(ID, 7, t1, 1000);
  expect(h1).toEqual({ t: [1000], cpu: [10], mem: [10e6], net: [100] });
  // Same table read again (a re-render): no new point.
  const h2 = recordProcessRow(ID, 7, t1, 2000);
  expect(h2.t).toEqual([1000]);
  const t2 = table(20);
  const h3 = recordProcessRow(ID, 7, t2, 3000);
  expect(h3.t).toEqual([1000, 3000]);
  expect(h3.cpu).toEqual([10, 20]);
  // Snapshots are independent copies.
  h3.cpu.push(99);
  expect(recordProcessRow(ID, 7, t2, 4000).cpu).toEqual([10, 20]);
  // An empty table appends nothing; a table without the pid appends a gap.
  expect(recordProcessRow(ID, 7, [], 5000).t).toEqual([1000, 3000]);
  const gap = recordProcessRow(ID, 7, [proc({ pid: 8, name: "b" })], 6000);
  expect(gap.t).toEqual([1000, 3000, 6000]);
  expect(Number.isNaN(gap.cpu[2])).toBe(true);
});

test("caps each ring at PROCESS_HISTORY_POINTS, dropping the oldest", () => {
  for (let i = 0; i < PROCESS_HISTORY_POINTS + 25; i += 1) recordProcessRow(ID, 7, table(i), i);
  const h = recordProcessRow(ID, 7, table(-1), 9_999_999);
  expect(h.t).toHaveLength(PROCESS_HISTORY_POINTS);
  expect(h.t[0]).toBe(26);
  expect(h.cpu[0]).toBe(26);
  expect(h.t[h.t.length - 1]).toBe(9_999_999);
  expect(processHistorySize(ID, 7)).toBe(PROCESS_HISTORY_POINTS);
});

test("keeps at most PROCESS_HISTORY_RINGS rings, evicting the least recently read", () => {
  for (let pid = 0; pid < PROCESS_HISTORY_RINGS; pid += 1) {
    recordProcessRow(ID, pid, [proc({ pid, name: "p", cpu_pct: 1 })], pid);
  }
  // Touch pid 0 so it is the most recent; then a new pid evicts pid 1.
  recordProcessRow(ID, 0, [proc({ pid: 0, name: "p", cpu_pct: 2 })], 500);
  recordProcessRow(ID, 1000, [proc({ pid: 1000, name: "p", cpu_pct: 1 })], 501);
  expect(processHistorySize(ID, 0)).toBe(2);
  expect(processHistorySize(ID, 1)).toBe(0);
  expect(processHistorySize(ID, 1000)).toBe(1);
});

test("rings are keyed by device and pid", () => {
  const t1 = table(1);
  const t2 = table(2);
  recordProcessRow(ID, 7, t1, 1);
  recordProcessRow("fake:ipad", 7, t2, 2);
  expect(recordProcessRow(ID, 7, t1, 3).cpu).toEqual([1]);
  expect(recordProcessRow("fake:ipad", 7, t2, 3).cpu).toEqual([2]);
});

test("the hook appends on each processes tick", async () => {
  processes(ID, table(5));
  const { result } = renderHook(() => useProcessHistory(ID, 7));
  expect(result.current.cpu).toEqual([5]);
  act(() => processes(ID, table(6)));
  await act(async () => {
    await new Promise<void>((r) => requestAnimationFrame(() => r()));
  });
  expect(result.current.cpu).toEqual([5, 6]);
  expect(result.current.mem).toEqual([5e6, 6e6]);
});

import { expect, test } from "vitest";
import { proc } from "../../overview/cards/testkit";
import {
  arrangeRows,
  compareRows,
  DEFAULT_SORT,
  isAppUnderTest,
  matchesSearch,
  maxCpu,
  nextSort,
} from "./processModel";

const ROWS = [
  proc({
    pid: 1842,
    name: "FieldKit",
    app_id: "com.example.fieldkit",
    cpu_pct: 19.7,
    mem_bytes: 434e6,
    threads: 31,
    net_bps: 2.3e6,
    state: "foreground",
    energy: "High",
  }),
  proc({
    pid: 1207,
    name: "Safari",
    app_id: "com.apple.mobilesafari",
    cpu_pct: 5.7,
    mem_bytes: 283e6,
    threads: 22,
    net_bps: 6.6e5,
    state: "background",
    energy: "Low",
  }),
  proc({ pid: 0, name: "kernel_task", cpu_pct: 6.5, mem_bytes: 1246e6, threads: 240, net_bps: 0 }),
  proc({
    pid: 84,
    name: "mediaserverd",
    cpu_pct: null,
    mem_bytes: 99e6,
    threads: 28,
    net_bps: null,
    state: "background",
  }),
  proc({
    pid: 61,
    name: "SpringBoard",
    app_id: "com.apple.springboard",
    cpu_pct: 2.2,
    mem_bytes: null,
    threads: 40,
    net_bps: 0,
    state: "background",
    energy: "Low",
  }),
];

const names = (rows: ReturnType<typeof arrangeRows>) => rows.map((r) => r.name);
const none = new Set<number>();

test("default sort is CPU descending with unknown CPU last", () => {
  expect(names(arrangeRows(ROWS, { sort: DEFAULT_SORT, query: "", pinned: none }))).toEqual([
    "FieldKit",
    "kernel_task",
    "Safari",
    "SpringBoard",
    "mediaserverd",
  ]);
});

test("nulls sort last in either direction; ties break on name then pid", () => {
  const asc = arrangeRows(ROWS, { sort: { key: "cpu_pct", dir: "asc" }, query: "", pinned: none });
  expect(names(asc)).toEqual(["SpringBoard", "Safari", "kernel_task", "FieldKit", "mediaserverd"]);
  const byMem = arrangeRows(ROWS, {
    sort: { key: "mem_bytes", dir: "desc" },
    query: "",
    pinned: none,
  });
  expect(names(byMem)).toEqual([
    "kernel_task",
    "FieldKit",
    "Safari",
    "mediaserverd",
    "SpringBoard",
  ]);
  const a = proc({ pid: 2, name: "same", cpu_pct: 1 });
  const b = proc({ pid: 1, name: "same", cpu_pct: 1 });
  expect(compareRows(a, b, DEFAULT_SORT)).toBeGreaterThan(0);
});

test("text, state and energy columns sort by rank", () => {
  const byName = arrangeRows(ROWS, { sort: { key: "name", dir: "asc" }, query: "", pinned: none });
  expect(names(byName)).toEqual([
    "FieldKit",
    "kernel_task",
    "mediaserverd",
    "Safari",
    "SpringBoard",
  ]);
  const byState = arrangeRows(ROWS, {
    sort: { key: "state", dir: "asc" },
    query: "",
    pinned: none,
  });
  expect(byState[0].state).toBe("foreground");
  expect(byState[byState.length - 1].state).toBe("unknown");
  const byEnergy = arrangeRows(ROWS, {
    sort: { key: "energy", dir: "desc" },
    query: "",
    pinned: none,
  });
  expect(byEnergy[0].energy).toBe("High");
  expect(byEnergy.slice(-2).every((r) => r.energy === null)).toBe(true);
  const byId = arrangeRows(ROWS, { sort: { key: "app_id", dir: "asc" }, query: "", pinned: none });
  expect(byId.slice(-2).every((r) => r.app_id === null)).toBe(true);
});

test("nextSort flips the same column and starts numeric columns high-first", () => {
  expect(nextSort(DEFAULT_SORT, "cpu_pct")).toEqual({ key: "cpu_pct", dir: "asc" });
  expect(nextSort(DEFAULT_SORT, "mem_bytes")).toEqual({ key: "mem_bytes", dir: "desc" });
  expect(nextSort(DEFAULT_SORT, "name")).toEqual({ key: "name", dir: "asc" });
  expect(nextSort({ key: "name", dir: "asc" }, "name")).toEqual({ key: "name", dir: "desc" });
});

test("search matches name or app id, case-insensitively", () => {
  expect(matchesSearch(ROWS[0], "field")).toBe(true);
  expect(matchesSearch(ROWS[1], "MOBILESAFARI")).toBe(true);
  expect(matchesSearch(ROWS[2], "safari")).toBe(false);
  expect(matchesSearch(ROWS[2], "  ")).toBe(true);
  expect(
    names(arrangeRows(ROWS, { sort: DEFAULT_SORT, query: "com.apple", pinned: none })),
  ).toEqual(["Safari", "SpringBoard"]);
});

test("pinned rows come first, in sort order among themselves", () => {
  const pinned = new Set([61, 84]);
  expect(names(arrangeRows(ROWS, { sort: DEFAULT_SORT, query: "", pinned }))).toEqual([
    "SpringBoard",
    "mediaserverd",
    "FieldKit",
    "kernel_task",
    "Safari",
  ]);
});

test("the app under test is FieldKit by app id or name", () => {
  expect(isAppUnderTest(ROWS[0])).toBe(true);
  expect(isAppUnderTest(proc({ name: "com.example.fieldkit" }))).toBe(true);
  expect(isAppUnderTest(ROWS[1])).toBe(false);
  expect(maxCpu(ROWS)).toBe(19.7);
  expect(maxCpu([proc({ name: "x" })])).toBe(0);
});

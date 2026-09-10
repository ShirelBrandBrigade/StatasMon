import type { LogFilter } from "@statasmon/protocol";
import { expect, test } from "vitest";
import type { FilteredLog } from "../../overview/log";
import { entry, mockupEntries } from "../../overview/log/testEntries";
import { LogRing } from "../../store/telemetry";
import {
  activeTabId,
  appTabs,
  BufferTracker,
  EMPTY_STORED,
  narrowLog,
  nextStoredPage,
  parseLocalDateTime,
  reportCount,
  sortBuffers,
  storedFilter,
  storedKey,
  storedRemaining,
  toLocalDateTime,
} from "./logsModel";

function filtered(entries = mockupEntries()): FilteredLog {
  return {
    entries,
    keys: entries.map((_, i) => i),
    total: entries.length,
    shown: entries.length,
    errors: entries.filter((e) => e.level === "error").length,
    faults: entries.filter((e) => e.level === "fault").length,
    dropped: 0,
    invalid: false,
    processes: [],
    warningsSeen: true,
  };
}

test("buffers sort in logcat order, then alphabetically", () => {
  expect(sortBuffers(["zeta", "system", "alpha", "main", "crash"])).toEqual([
    "main",
    "system",
    "crash",
    "alpha",
    "zeta",
  ]);
});

test("BufferTracker scans only new lines and keeps the array stable while unchanged", () => {
  const ring = new LogRing(4);
  const tracker = new BufferTracker();
  expect(tracker.update(ring)).toEqual([]);
  ring.push([entry(1, "a", "info", "x"), entry(2, "a", "info", "x", { buffer: "system" })]);
  const first = tracker.update(ring);
  expect(first).toEqual(["system"]);
  ring.push([entry(3, "a", "info", "x", { buffer: "system" })]);
  expect(tracker.update(ring)).toBe(first);
  // Dropping lines from the front does not lose buffers already seen.
  ring.push([entry(4, "a", "info", "x", { buffer: "main" }), entry(5, "a", "info", "x")]);
  expect(ring.droppedCount).toBe(1);
  expect(tracker.update(ring)).toEqual(["main", "system"]);
  // A new ring (detach) starts over.
  expect(tracker.update(new LogRing())).toEqual([]);
});

test("narrowLog returns the same object without narrowing and filters by time and buffer", () => {
  const log = filtered();
  expect(narrowLog(log, { buffers: [] })).toBe(log);
  const byTime = narrowLog(log, {
    fromMs: log.entries[2].ts_ms,
    toMs: log.entries[4].ts_ms,
    buffers: [],
  });
  expect(byTime.entries.map((e) => e.seq)).toEqual([3, 4, 5]);
  expect(byTime.keys).toEqual([2, 3, 4]);
  expect(byTime).toMatchObject({ shown: 3, errors: 1, faults: 0, total: 8 });
  const byBuffer = narrowLog(log, { buffers: ["main"] });
  expect(byBuffer.entries.map((e) => e.seq)).toEqual([8]);
  expect(narrowLog(log, { buffers: ["crash"] }).entries).toEqual([]);
});

test("datetime-local round-trips in local time", () => {
  const ms = new Date(2026, 8, 9, 14, 5, 9).getTime();
  expect(toLocalDateTime(ms)).toBe("2026-09-09T14:05:09");
  expect(parseLocalDateTime("2026-09-09T14:05:09")).toBe(ms);
  expect(parseLocalDateTime("2026-09-09T14:05")).toBe(ms - 9000);
  expect(parseLocalDateTime("2026-09-09T14:05:09.000")).toBe(ms);
  expect(parseLocalDateTime("")).toBeUndefined();
  expect(parseLocalDateTime("yesterday")).toBeUndefined();
});

test("app tabs: All, the app under test, running apps that logged, and the selection", () => {
  const counts = [
    { process: "system_server", count: 40 },
    { process: "adbd", count: 9 },
    { process: "logd", count: 8 },
    { process: "com.example.maps", count: 7 },
    { process: "surfaceflinger", count: 6 },
    { process: "com.example.fieldkit", count: 2 },
  ];
  const isMe = (p: string) => p.includes("fieldkit");
  const apps = ["com.example.maps", "com.example.fieldkit", "com.example.idle"];
  // System daemons never get a tab; the app under test comes first.
  const tabs = appTabs(counts, isMe, [], apps);
  expect(tabs.map((t) => t.label)).toEqual(["All", "com.example.fieldkit", "com.example.maps"]);
  expect(tabs[1].me).toBe(true);
  expect(tabs[2].me).toBe(false);
  // A selected daemon gets a tab while selected, once.
  expect(appTabs(counts, isMe, ["adbd", "com.example.maps"], apps).map((t) => t.label)).toEqual([
    "All",
    "com.example.fieldkit",
    "com.example.maps",
    "adbd",
  ]);
  // Without a process table only the app under test and the selection show.
  expect(appTabs(counts, isMe, ["logd"]).map((t) => t.label)).toEqual([
    "All",
    "com.example.fieldkit",
    "logd",
  ]);
  expect(activeTabId(tabs, [])).toBe("all");
  expect(activeTabId(tabs, ["com.example.maps"])).toBe("p:com.example.maps");
  expect(activeTabId(tabs, ["adbd", "logd"])).toBeUndefined();
  expect(activeTabId(tabs, ["unknown"])).toBeUndefined();
});

test("stored paging walks backwards from the end of the matches", () => {
  expect(nextStoredPage(EMPTY_STORED)).toEqual({ offset: 0, limit: 500 });
  const probed = { ...EMPTY_STORED, total: 1_234 };
  expect(nextStoredPage(probed)).toEqual({ offset: 734, limit: 500 });
  expect(storedRemaining(probed)).toBe(1_234);
  const one = { ...probed, entries: [entry(1, "a", "info", "x")], offset: 734 };
  expect(nextStoredPage(one)).toEqual({ offset: 234, limit: 500 });
  expect(storedRemaining(one)).toBe(734);
  const two = { ...one, offset: 234 };
  expect(nextStoredPage(two)).toEqual({ offset: 0, limit: 234 });
  expect(nextStoredPage({ ...two, offset: 0 })).toBeUndefined();
  expect(storedRemaining({ ...two, offset: 0 })).toBe(0);
  expect(nextStoredPage({ ...EMPTY_STORED, total: 0 })).toBeUndefined();
  expect(storedKey(734, 0)).toBe(-735);
  expect(storedKey(734, 1)).not.toBe(storedKey(734, 0));
});

test("storedFilter bounds the query to lines older than the ring and carries the narrowing", () => {
  const base: LogFilter = {
    device_id: "d",
    session_id: "s",
    processes: [],
    levels: ["info"],
    query: null,
    regex: false,
    from_ms: null,
    to_ms: null,
    buffers: [],
  };
  expect(storedFilter(base, { buffers: ["main"] }, 10_000)).toMatchObject({
    to_ms: 9_999,
    from_ms: null,
    buffers: ["main"],
  });
  expect(storedFilter(base, { fromMs: 5, toMs: 20_000, buffers: [] }, 10_000)).toMatchObject({
    from_ms: 5,
    to_ms: 9_999,
  });
  expect(storedFilter(base, { toMs: 500, buffers: [] }, 10_000).to_ms).toBe(500);
  expect(storedFilter(base, { buffers: [] }, undefined).to_ms).toBeNull();
});

test("reportCount reads the shapes a fetch_crash_reports result may take", () => {
  expect(reportCount(3)).toBe(3);
  expect(reportCount(["a", "b"])).toBe(2);
  expect(reportCount({ count: 4 })).toBe(4);
  expect(reportCount({ reports: ["x"] })).toBe(1);
  expect(reportCount(null)).toBeUndefined();
  expect(reportCount({ ok: true })).toBeUndefined();
});

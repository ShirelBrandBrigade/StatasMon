import { act, renderHook } from "@testing-library/react";
import { beforeEach, expect, test } from "vitest";
import { dispatch } from "../../bridge/dispatch";
import { useAppStore } from "../../store/app";
import { useLogFilterStore } from "../../store/logFilter";
import { telemetry } from "../../store/telemetry";
import { mockupEntries } from "./testEntries";
import { useFilteredLog } from "./useFilteredLog";

const ID = "fake:iphone";

beforeEach(() => {
  useAppStore.getState().reset();
  useLogFilterStore.getState().reset();
  telemetry.reset();
});

/** Lets the rAF-throttled tick fire. */
async function frame() {
  await act(async () => {
    await new Promise<void>((r) => requestAnimationFrame(() => r()));
  });
}

test("appends incrementally across log batches and refilters on a filter change", async () => {
  const rows = mockupEntries();
  const { result } = renderHook(() => useFilteredLog(ID));
  expect(result.current).toMatchObject({ total: 0, shown: 0 });

  act(() => dispatch({ type: "log", device_id: ID, entries: rows.slice(0, 3) }));
  await frame();
  const first = result.current;
  expect(first.entries.map((e) => e.seq)).toEqual([1, 2, 3]);
  expect(first).toMatchObject({ total: 3, shown: 3, errors: 1, faults: 0 });

  act(() => dispatch({ type: "log", device_id: ID, entries: rows.slice(3) }));
  await frame();
  const second = result.current;
  expect(second.entries).not.toBe(first.entries);
  expect(second.entries.map((e) => e.seq)).toEqual([1, 2, 3, 4, 5, 7, 8]);
  expect(second).toMatchObject({ total: 8, shown: 7, errors: 1, faults: 1 });
  expect(second.processes.map((p) => p.process)).toEqual([
    "FieldKit",
    "com.example.fieldkit",
    "Safari",
    "SpringBoard",
    "thermalmonitord",
  ]);

  act(() => useLogFilterStore.getState().toggleLevel(ID, "debug"));
  expect(result.current.entries.map((e) => e.seq)).toEqual([1, 2, 3, 4, 5, 6, 7, 8]);

  act(() => useLogFilterStore.getState().setProcesses(ID, ["FieldKit"]));
  expect(result.current.entries.map((e) => e.seq)).toEqual([1, 3, 4, 7]);
  expect(result.current).toMatchObject({ total: 8, shown: 4, errors: 1, faults: 1 });

  // followTail is not a matching field: no rescan, same entries reference.
  const before = result.current.entries;
  act(() => useLogFilterStore.getState().setFollowTail(ID, false));
  expect(result.current.entries).toBe(before);
});

test("other devices' batches do not touch this device", async () => {
  const { result } = renderHook(() => useFilteredLog(ID));
  act(() => dispatch({ type: "log", device_id: "fake:ipad", entries: mockupEntries() }));
  await frame();
  expect(result.current.total).toBe(0);
});
